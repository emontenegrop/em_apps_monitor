use std::{sync::Arc, time::Duration};

use axum::{
    extract::{Path, State},
    Json,
};
use em_protocol::messages::{RequestMethod, CLOSE_REVOKED};
use uuid::Uuid;

use crate::{
    error::{ApiResult, AppError},
    models::server::{CreateServer, Server, UpdateServer},
    state::AppState,
};

pub async fn list_servers(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<Server>>> {
    let servers =
        sqlx::query_as::<_, Server>("SELECT * FROM servers ORDER BY created_at ASC")
            .fetch_all(&state.db)
            .await?;
    Ok(Json(servers))
}

pub async fn get_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Server>> {
    let server = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Server {id} not found")))?;
    Ok(Json(server))
}

/// Crea el servidor; queda esperando que su agente se inscriba con una ficha
pub async fn create_server(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateServer>,
) -> ApiResult<Json<Server>> {
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let tags = serde_json::to_value(payload.tags.unwrap_or_default())?;

    let server = sqlx::query_as::<_, Server>(
        "INSERT INTO servers (name, tags) VALUES ($1, $2) RETURNING *",
    )
    .bind(name)
    .bind(tags)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(server))
}

pub async fn update_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateServer>,
) -> ApiResult<Json<Server>> {
    if payload.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
        return Err(AppError::BadRequest("name cannot be empty".into()));
    }
    let tags = payload.tags.map(serde_json::to_value).transpose()?;

    let server = sqlx::query_as::<_, Server>(
        r#"UPDATE servers SET
             name = COALESCE($2, name), tags = COALESCE($3, tags),
             is_active = COALESCE($4, is_active), updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(payload.name.as_deref().map(str::trim))
    .bind(tags)
    .bind(payload.is_active)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Server {id} not found")))?;

    Ok(Json(server))
}

pub async fn delete_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    state.agents.disconnect(id, CLOSE_REVOKED, "servidor eliminado").await;
    let res = sqlx::query("DELETE FROM servers WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Server {id} not found")));
    }
    Ok(Json(serde_json::json!({ "deleted": id })))
}

/// Ida y vuelta con el agente del servidor, con su latencia
pub async fn ping_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM servers WHERE id = $1)")
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    if !exists {
        return Err(AppError::NotFound(format!("Server {id} not found")));
    }

    let started = std::time::Instant::now();
    let res = state.agents.request(id, RequestMethod::Ping, Duration::from_secs(5)).await;
    Ok(Json(match res {
        Ok(_) => serde_json::json!({
            "status": "online",
            "via": "agent",
            "latency_ms": started.elapsed().as_millis() as u64,
        }),
        Err(e) => serde_json::json!({ "status": "offline", "via": "agent", "error": e.to_string() }),
    }))
}
