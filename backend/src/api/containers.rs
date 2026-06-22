use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    collectors::docker::{list_images, list_networks, list_volumes},
    error::{ApiResult, AppError},
    models::container::Container,
    state::AppState,
};

#[derive(Deserialize)]
pub struct ContainerFilter {
    pub state: Option<String>, // running | exited | all
}

pub async fn list_containers(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
    Query(q): Query<ContainerFilter>,
) -> ApiResult<Json<Vec<Container>>> {
    let containers = match q.state.as_deref() {
        Some(s) if s != "all" => {
            sqlx::query_as::<_, Container>(
                "SELECT * FROM containers WHERE server_id = $1 AND state = $2 ORDER BY name",
            )
            .bind(server_id)
            .bind(s)
            .fetch_all(&state.db)
            .await?
        }
        _ => {
            sqlx::query_as::<_, Container>(
                "SELECT * FROM containers WHERE server_id = $1 ORDER BY name",
            )
            .bind(server_id)
            .fetch_all(&state.db)
            .await?
        }
    };
    Ok(Json(containers))
}

pub async fn get_container(
    State(state): State<Arc<AppState>>,
    Path((server_id, container_id)): Path<(Uuid, String)>,
) -> ApiResult<Json<Container>> {
    let c = sqlx::query_as::<_, Container>(
        "SELECT * FROM containers WHERE id = $1 AND server_id = $2",
    )
    .bind(&container_id)
    .bind(server_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Container {container_id} not found")))?;
    Ok(Json(c))
}

/// Inspect returns the raw Docker inspect JSON
pub async fn inspect_container(
    State(state): State<Arc<AppState>>,
    Path((server_id, container_id)): Path<(Uuid, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let docker = get_docker_client(&state, server_id).await?;
    let details = docker.inspect_container(&container_id, None).await?;
    Ok(Json(serde_json::to_value(details)?))
}

pub async fn container_logs(
    State(state): State<Arc<AppState>>,
    Path((server_id, container_id)): Path<(Uuid, String)>,
) -> ApiResult<Json<Vec<String>>> {
    use bollard::container::LogsOptions;
    use futures_util::StreamExt;

    let docker = get_docker_client(&state, server_id).await?;
    let mut stream = docker.logs(
        &container_id,
        Some(LogsOptions::<String> {
            stdout: true,
            stderr: true,
            tail: "200".to_string(),
            timestamps: true,
            ..Default::default()
        }),
    );

    let mut lines = Vec::new();
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(output) => lines.push(output.to_string()),
            Err(_) => break,
        }
    }
    Ok(Json(lines))
}

pub async fn server_images(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let docker = get_docker_client(&state, server_id).await?;
    Ok(Json(list_images(&docker).await?))
}

pub async fn server_networks(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let docker = get_docker_client(&state, server_id).await?;
    Ok(Json(list_networks(&docker).await?))
}

pub async fn server_volumes(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let docker = get_docker_client(&state, server_id).await?;
    Ok(Json(list_volumes(&docker).await?))
}

// ── helpers ───────────────────────────────────────────────────────────────

async fn get_docker_client(
    state: &Arc<AppState>,
    server_id: Uuid,
) -> ApiResult<std::sync::Arc<bollard::Docker>> {
    state
        .docker_clients
        .read()
        .await
        .get(&server_id)
        .cloned()
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "No active Docker connection for server {server_id}. Try pinging the server first."
            ))
        })
}
