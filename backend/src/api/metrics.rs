use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    collectors::host::collect_disk_usage,
    error::ApiResult,
    models::metrics::{ContainerMetrics, DockerInfo, DiskUsage, HostMetrics},
    state::AppState,
};

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
        "SELECT * FROM docker_info WHERE server_id = $1",
    )
    .bind(server_id)
    .fetch_optional(&state.db)
    .await?;

    Ok(Json(info))
}

pub async fn disk_usage(
    State(_state): State<Arc<AppState>>,
    Path(_server_id): Path<Uuid>,
) -> ApiResult<Json<Vec<DiskUsage>>> {
    // For local servers, collect directly. Remote disk usage requires an agent.
    let disks = collect_disk_usage();
    Ok(Json(disks))
}
