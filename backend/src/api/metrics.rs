use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::ApiResult,
    models::metrics::{ContainerMetrics, DockerInfo, DiskUsage, HostMetrics, RuntimeInfo},
    state::AppState,
};

/// Período de las gráficas históricas (`24h`, `3d`, `5d`, `7d`)
#[derive(Deserialize)]
pub struct HistoryQuery {
    pub range: Option<String>,
    /// Solo este servidor (por defecto, todos)
    pub server_id: Option<Uuid>,
}

/// Duración del período y tamaño del intervalo de agregación, para ~300 puntos por serie
fn history_window(range: Option<&str>) -> (chrono::Duration, i64) {
    match range {
        Some("3d") => (chrono::Duration::days(3), 15 * 60),
        Some("5d") => (chrono::Duration::days(5), 30 * 60),
        Some("7d") => (chrono::Duration::days(7), 30 * 60),
        _ => (chrono::Duration::hours(24), 5 * 60),
    }
}

#[derive(Serialize)]
pub struct History<T> {
    /// Tamaño del intervalo en segundos (para detectar huecos)
    pub bucket_secs: i64,
    pub points: Vec<T>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct HostHistoryPoint {
    pub server_id: Uuid,
    pub time: DateTime<Utc>,
    pub cpu_percent: Option<f64>,
    pub memory_used_bytes: Option<f64>,
    pub memory_total_bytes: Option<f64>,
    pub disk_used_bytes: Option<f64>,
    pub disk_total_bytes: Option<f64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ContainerHistoryPoint {
    pub container_id: String,
    pub name: String,
    pub time: DateTime<Utc>,
    pub cpu_percent: Option<f64>,
    pub memory_used_bytes: Option<f64>,
    /// Lectura + escritura en disco (bytes/s)
    pub disk_io_bytes: Option<f64>,
}

/// CPU, memoria y disco de los servidores, promediados por intervalo
pub async fn host_history(
    State(state): State<Arc<AppState>>,
    Query(q): Query<HistoryQuery>,
) -> ApiResult<Json<History<HostHistoryPoint>>> {
    let (span, bucket_secs) = history_window(q.range.as_deref());
    let points = sqlx::query_as::<_, HostHistoryPoint>(
        r#"WITH h AS (
               SELECT server_id,
                      date_bin(make_interval(secs => $1), time, TIMESTAMPTZ 'epoch') AS time,
                      avg(cpu_usage_percent)                  AS cpu_percent,
                      avg(memory_used_bytes)::float8          AS memory_used_bytes,
                      max(memory_total_bytes)::float8         AS memory_total_bytes
                 FROM host_metrics
                WHERE time >= $2 AND ($3::uuid IS NULL OR server_id = $3)
                GROUP BY 1, 2
           ), d AS (
               SELECT server_id,
                      date_bin(make_interval(secs => $1), time, TIMESTAMPTZ 'epoch') AS time,
                      avg(used_bytes)::float8                 AS disk_used_bytes,
                      max(total_bytes)::float8                AS disk_total_bytes
                 FROM disk_metrics
                WHERE time >= $2 AND ($3::uuid IS NULL OR server_id = $3)
                GROUP BY 1, 2
           )
           SELECT server_id, time, h.cpu_percent, h.memory_used_bytes, h.memory_total_bytes,
                  d.disk_used_bytes, d.disk_total_bytes
             FROM h FULL JOIN d USING (server_id, time)
            ORDER BY time"#,
    )
    .bind(bucket_secs as f64)
    .bind(Utc::now() - span)
    .bind(q.server_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(History { bucket_secs, points }))
}

/// CPU, memoria e I/O de disco de cada contenedor del servidor, promediados por intervalo
pub async fn containers_history(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
    Query(q): Query<HistoryQuery>,
) -> ApiResult<Json<History<ContainerHistoryPoint>>> {
    let (span, bucket_secs) = history_window(q.range.as_deref());
    let points = sqlx::query_as::<_, ContainerHistoryPoint>(
        r#"SELECT m.container_id,
                  COALESCE(c.name, left(m.container_id, 12))                       AS name,
                  date_bin(make_interval(secs => $1), m.time, TIMESTAMPTZ 'epoch') AS time,
                  avg(m.cpu_usage_percent)                                         AS cpu_percent,
                  avg(m.memory_used_bytes)::float8                                 AS memory_used_bytes,
                  (COALESCE(avg(m.block_read_bytes), 0)
                   + COALESCE(avg(m.block_write_bytes), 0))::float8               AS disk_io_bytes
             FROM container_metrics m
             LEFT JOIN containers c ON c.id = m.container_id AND c.server_id = m.server_id
            WHERE m.server_id = $3 AND m.time >= $2
            GROUP BY 1, 2, 3
            ORDER BY 3"#,
    )
    .bind(bucket_secs as f64)
    .bind(Utc::now() - span)
    .bind(server_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(History { bucket_secs, points }))
}

#[derive(Deserialize)]
pub struct TimeRange {
    /// ISO 8601 — defaults to 1 hour ago
    pub from: Option<String>,
    /// ISO 8601 — defaults to now
    pub to: Option<String>,
    /// Maximum number of data points to return
    pub limit: Option<i64>,
}

pub async fn host_metrics(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
    Query(q): Query<TimeRange>,
) -> ApiResult<Json<Vec<HostMetrics>>> {
    let from = q
        .from
        .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::hours(1));
    let to = q
        .to
        .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
        .unwrap_or_else(chrono::Utc::now);
    let limit = q.limit.unwrap_or(300).min(2000);

    let rows = sqlx::query_as::<_, HostMetrics>(
        r#"SELECT * FROM host_metrics
           WHERE server_id = $1 AND time BETWEEN $2 AND $3
           ORDER BY time ASC
           LIMIT $4"#,
    )
    .bind(server_id)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

pub async fn container_metrics(
    State(state): State<Arc<AppState>>,
    Path((_server_id, container_id)): Path<(Uuid, String)>,
    Query(q): Query<TimeRange>,
) -> ApiResult<Json<Vec<ContainerMetrics>>> {
    let from = q
        .from
        .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::hours(1));
    let to = q
        .to
        .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
        .unwrap_or_else(chrono::Utc::now);
    let limit = q.limit.unwrap_or(300).min(2000);

    let rows = sqlx::query_as::<_, ContainerMetrics>(
        r#"SELECT * FROM container_metrics
           WHERE container_id = $1 AND time BETWEEN $2 AND $3
           ORDER BY time ASC
           LIMIT $4"#,
    )
    .bind(&container_id)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

pub async fn docker_info(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
) -> ApiResult<Json<Option<DockerInfo>>> {
    let info = sqlx::query_as::<_, DockerInfo>(
        "SELECT * FROM docker_info WHERE server_id = $1 ORDER BY runtime_kind LIMIT 1",
    )
    .bind(server_id)
    .fetch_optional(&state.db)
    .await?;

    Ok(Json(info))
}

/// Info de cada runtime del servidor (Docker y/o Podman)
pub async fn runtimes(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
) -> ApiResult<Json<Vec<RuntimeInfo>>> {
    let rows = sqlx::query_as::<_, RuntimeInfo>(
        "SELECT * FROM docker_info WHERE server_id = $1 ORDER BY runtime_kind",
    )
    .bind(server_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

pub async fn disk_usage(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
) -> ApiResult<Json<Vec<DiskUsage>>> {
    let rows: Vec<(String, String, String, i64, i64, i64, f64)> = sqlx::query_as(
        r#"SELECT name, mount_point, fs_type, total_bytes, used_bytes, available_bytes, usage_percent
           FROM disk_usage_latest WHERE server_id = $1 ORDER BY mount_point"#,
    )
    .bind(server_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|(name, mount_point, fs_type, total, used, available, pct)| DiskUsage {
                name,
                mount_point,
                total_bytes: total as u64,
                used_bytes: used as u64,
                available_bytes: available as u64,
                usage_percent: pct,
                fs_type,
            })
            .collect(),
    ))
}
