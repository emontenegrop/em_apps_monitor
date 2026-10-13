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
    collectors::docker::{list_images, list_networks, list_volumes},
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

    if has_agent(&state, server_id).await? {
        let runtime = container_runtime(&state, server_id, &container_id).await?;
        let method = RequestMethod::Inspect { runtime, container_id, reveal_env: q.reveal_env };
        return Ok(Json(state.agents.request(server_id, method, AGENT_TIMEOUT).await?));
    }

    let docker = get_docker_client(&state, server_id).await?;
    let details = docker.inspect_container(&container_id, None).await?;
    let mut value = serde_json::to_value(details)?;
    if !q.reveal_env {
        em_protocol::redact::redact_inspect_env(&mut value);
    }
    Ok(Json(value))
}

pub async fn container_logs(
    State(state): State<Arc<AppState>>,
    Path((server_id, container_id)): Path<(Uuid, String)>,
    Query(q): Query<LogsQuery>,
) -> ApiResult<Json<Vec<String>>> {
    let tail = q.tail.unwrap_or(200).clamp(1, MAX_LOG_TAIL);

    if has_agent(&state, server_id).await? {
        let runtime = container_runtime(&state, server_id, &container_id).await?;
        let value = state
            .agents
            .request(server_id, RequestMethod::Logs { runtime, container_id, tail }, AGENT_TIMEOUT)
            .await?;
        let lines: Vec<String> = serde_json::from_value(value)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("logs del agente con formato inválido: {e}")))?;
        return Ok(Json(lines));
    }

    use bollard::container::LogsOptions;
    use futures_util::StreamExt;

    let docker = get_docker_client(&state, server_id).await?;
    let mut stream = docker.logs(
        &container_id,
        Some(LogsOptions::<String> {
            stdout: true,
            stderr: true,
            tail: tail.to_string(),
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
    Query(q): Query<RuntimeQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    if has_agent(&state, server_id).await? {
        let runtime = requested_runtime(&state, server_id, q.runtime.as_deref()).await?;
        return Ok(Json(
            state.agents.request(server_id, RequestMethod::Images { runtime }, AGENT_TIMEOUT).await?,
        ));
    }
    let docker = get_docker_client(&state, server_id).await?;
    Ok(Json(list_images(&docker).await?))
}

pub async fn server_networks(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
    Query(q): Query<RuntimeQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    if has_agent(&state, server_id).await? {
        let runtime = requested_runtime(&state, server_id, q.runtime.as_deref()).await?;
        return Ok(Json(
            state.agents.request(server_id, RequestMethod::Networks { runtime }, AGENT_TIMEOUT).await?,
        ));
    }
    let docker = get_docker_client(&state, server_id).await?;
    Ok(Json(list_networks(&docker).await?))
}

pub async fn server_volumes(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<Uuid>,
    Query(q): Query<RuntimeQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    if has_agent(&state, server_id).await? {
        let runtime = requested_runtime(&state, server_id, q.runtime.as_deref()).await?;
        return Ok(Json(
            state.agents.request(server_id, RequestMethod::Volumes { runtime }, AGENT_TIMEOUT).await?,
        ));
    }
    let docker = get_docker_client(&state, server_id).await?;
    Ok(Json(list_volumes(&docker).await?))
}

// ── helpers ───────────────────────────────────────────────────────────────

/// ¿El servidor lo monitorea un agente? (si no, se usa el recolector central)
async fn has_agent(state: &Arc<AppState>, server_id: Uuid) -> ApiResult<bool> {
    sqlx::query_scalar::<_, bool>("SELECT uses_agent FROM servers WHERE id = $1")
        .bind(server_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Server {server_id} not found")))
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
