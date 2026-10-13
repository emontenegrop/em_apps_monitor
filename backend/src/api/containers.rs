use std::{sync::Arc, time::Duration};

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use em_protocol::messages::{RequestMethod, RuntimeKind};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{ApiResult, AppError},
    models::container::Container,
    state::AppState,
};

/// Tiempo máximo de espera de una solicitud al agente
const AGENT_TIMEOUT: Duration = Duration::from_secs(15);
/// Máximo de líneas de log que se piden
const MAX_LOG_TAIL: u32 = 1000;

#[derive(Deserialize)]
pub struct ContainerFilter {
    pub state: Option<String>, // running | exited | all
}

#[derive(Deserialize)]
pub struct RuntimeQuery {
    /// docker | podman. Por defecto, el primer runtime del servidor.
    pub runtime: Option<String>,
}

#[derive(Deserialize)]
pub struct InspectQuery {
    /// Muestra los valores de las variables de entorno (por defecto se ocultan)
    #[serde(default)]
    pub reveal_env: bool,
}

#[derive(Deserialize)]
pub struct LogsQuery {
    pub tail: Option<u32>,
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
    Extension(user): Extension<CurrentUser>,
    Path((server_id, container_id)): Path<(Uuid, String)>,
    Query(q): Query<InspectQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    if q.reveal_env {
        tracing::warn!("'{}' pidió las variables de entorno de {container_id} en {server_id}", user.username);
    }

    let runtime = container_runtime(&state, server_id, &container_id).await?;
    let method = RequestMethod::Inspect { runtime, container_id, reveal_env: q.reveal_env };
    Ok(Json(state.agents.request(server_id, method, AGENT_TIMEOUT).await?))
}

pub async fn container_logs(
    State(state): State<Arc<AppState>>,
    Path((server_id, container_id)): Path<(Uuid, String)>,
    Query(q): Query<LogsQuery>,
) -> ApiResult<Json<Vec<String>>> {
    let tail = q.tail.unwrap_or(200).clamp(1, MAX_LOG_TAIL);

    let runtime = container_runtime(&state, server_id, &container_id).await?;
    let value = state
        .agents
        .request(server_id, RequestMethod::Logs { runtime, container_id, tail }, AGENT_TIMEOUT)
        .await?;
    let lines: Vec<String> = serde_json::from_value(value)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("logs del agente con formato inválido: {e}")))?;
    Ok(Json(lines))
}

pub async fn server_images(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
    Query(q): Query<RuntimeQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let runtime = requested_runtime(&state, server_id, q.runtime.as_deref()).await?;
    Ok(Json(state.agents.request(server_id, RequestMethod::Images { runtime }, AGENT_TIMEOUT).await?))
}

pub async fn server_networks(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
    Query(q): Query<RuntimeQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let runtime = requested_runtime(&state, server_id, q.runtime.as_deref()).await?;
    Ok(Json(state.agents.request(server_id, RequestMethod::Networks { runtime }, AGENT_TIMEOUT).await?))
}

pub async fn server_volumes(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
    Query(q): Query<RuntimeQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let runtime = requested_runtime(&state, server_id, q.runtime.as_deref()).await?;
    Ok(Json(state.agents.request(server_id, RequestMethod::Volumes { runtime }, AGENT_TIMEOUT).await?))
}

// ── helpers ───────────────────────────────────────────────────────────────

/// 404 si el servidor no existe
async fn ensure_server(state: &Arc<AppState>, server_id: Uuid) -> ApiResult<()> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM servers WHERE id = $1)")
        .bind(server_id)
        .fetch_one(&state.db)
        .await?;
    if exists {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("Server {server_id} not found")))
    }
}

fn parse_runtime(name: &str) -> Option<RuntimeKind> {
    match name {
        "docker" => Some(RuntimeKind::Docker),
        "podman" => Some(RuntimeKind::Podman),
        _ => None,
    }
}

/// Runtime de un contenedor según el último listado. Acepta id, id corto o nombre,
/// igual que la API de Docker.
async fn container_runtime(state: &Arc<AppState>, server_id: Uuid, container: &str) -> ApiResult<RuntimeKind> {
    ensure_server(state, server_id).await?;
    let kind: Option<String> = sqlx::query_scalar(
        r#"SELECT runtime_kind FROM containers
           WHERE server_id = $1 AND (id = $2 OR short_id = $2 OR name = $2)
           LIMIT 1"#,
    )
    .bind(server_id)
    .bind(container)
    .fetch_optional(&state.db)
    .await?;
    match kind.as_deref().and_then(parse_runtime) {
        Some(k) => Ok(k),
        // Contenedor recién creado que aún no aparece en el listado
        None => requested_runtime(state, server_id, None).await,
    }
}

/// Runtime pedido por query, o el primero que reportó el agente
async fn requested_runtime(state: &Arc<AppState>, server_id: Uuid, name: Option<&str>) -> ApiResult<RuntimeKind> {
    ensure_server(state, server_id).await?;
    if let Some(name) = name {
        return parse_runtime(name)
            .ok_or_else(|| AppError::BadRequest(format!("runtime desconocido: {name} (docker | podman)")));
    }
    let first: Option<String> =
        sqlx::query_scalar("SELECT runtimes -> 0 ->> 'kind' FROM servers WHERE id = $1")
            .bind(server_id)
            .fetch_optional(&state.db)
            .await?
            .flatten();
    first
        .as_deref()
        .and_then(parse_runtime)
        .ok_or_else(|| AppError::BadRequest("el agente no reportó ningún runtime".into()))
}
