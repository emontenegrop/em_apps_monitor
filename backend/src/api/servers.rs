use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use uuid::Uuid;

use em_protocol::messages::CLOSE_REVOKED;

use crate::{
    agents::auth,
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

pub async fn create_server(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateServer>,
) -> ApiResult<Json<Server>> {
    if payload.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let conn_type = payload
        .connection_type
        .as_deref()
        .unwrap_or("local")
        .to_string();
    let host = payload
        .host
        .clone()
        .unwrap_or_else(|| "localhost".to_string());
    let port = payload.port.unwrap_or(2375);
    let tls = payload.tls_enabled.unwrap_or(false);
    let tags = serde_json::to_value(payload.tags.unwrap_or_default())?;

    let server = sqlx::query_as::<_, Server>(
        r#"INSERT INTO servers (name, host, connection_type, docker_host, port, tls_enabled, tags, netdata_url)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING *"#,
    )
    .bind(&payload.name)
    .bind(&host)
    .bind(&conn_type)
    .bind(&payload.docker_host)
    .bind(port)
    .bind(tls)
    .bind(tags)
    .bind(&payload.netdata_url)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(server))
}

pub async fn update_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateServer>,
) -> ApiResult<Json<Server>> {
    // Verify exists
    let existing = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Server {id} not found")))?;

    let name = payload.name.unwrap_or(existing.name);
    let host = payload.host.unwrap_or(existing.host);
    let conn_type = payload.connection_type.unwrap_or(existing.connection_type);
    let docker_host = payload.docker_host.or(existing.docker_host);
    let netdata_url = payload.netdata_url.or(existing.netdata_url);
    let port = payload.port.unwrap_or(existing.port);
    let tls = payload.tls_enabled.unwrap_or(existing.tls_enabled);
    let is_active = payload.is_active.unwrap_or(existing.is_active);
    let tags = payload
        .tags
        .map(|t| serde_json::to_value(t).unwrap_or(serde_json::json!([])))
        .unwrap_or(existing.tags);

    // Invalidate cached client if connection changed
    state.docker_clients.write().await.remove(&id);

    let server = sqlx::query_as::<_, Server>(
        r#"UPDATE servers SET
             name=$2, host=$3, connection_type=$4, docker_host=$5, port=$6,
             tls_enabled=$7, tags=$8, is_active=$9, updated_at=$10, netdata_url=$11
           WHERE id=$1
           RETURNING *"#,
    )
    .bind(id)
    .bind(name)
    .bind(host)
    .bind(conn_type)
    .bind(docker_host)
    .bind(port)
    .bind(tls)
    .bind(tags)
    .bind(is_active)
    .bind(Utc::now())
    .bind(netdata_url)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(server))
}

pub async fn delete_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    state.docker_clients.write().await.remove(&id);
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

/// Re-check connectivity and update status immediately
pub async fn ping_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let server = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Server {id} not found")))?;

    // Servidor con agente: está en línea si su agente está conectado
    let has_agent: bool =
        sqlx::query_scalar("SELECT agent_secret_hash IS NOT NULL FROM servers WHERE id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await?;
    if has_agent {
        let status = if state.agents.is_connected(id).await { "online" } else { "offline" };
        return Ok(Json(serde_json::json!({ "status": status, "via": "agent" })));
    }

    // Remove any cached client so it rebuilds
    state.docker_clients.write().await.remove(&id);

    match crate::collectors::docker::make_docker_client(
        &server.connection_type,
        server.docker_host.as_deref(),
        &server.host,
        server.port,
    ) {
        Ok(client) => {
            match client.ping().await {
                Ok(_) => {
                    sqlx::query(
                        "UPDATE servers SET status='online', last_seen_at=NOW(), updated_at=NOW() WHERE id=$1",
                    )
                    .bind(id)
                    .execute(&state.db)
                    .await?;
                    state.docker_clients.write().await.insert(id, client);
                    Ok(Json(serde_json::json!({ "status": "online" })))
                }
                Err(e) => {
                    sqlx::query(
                        "UPDATE servers SET status='offline', updated_at=NOW() WHERE id=$1",
                    )
                    .bind(id)
                    .execute(&state.db)
                    .await?;
                    Ok(Json(serde_json::json!({ "status": "offline", "error": e.to_string() })))
                }
            }
        }
        Err(e) => Ok(Json(
            serde_json::json!({ "status": "offline", "error": e.to_string() }),
        )),
    }
}

/// Genera una credencial nueva para el agente del servidor. La anterior deja de valer
/// y, si había un agente conectado con ella, se lo desconecta.
///
/// El secreto se devuelve una sola vez; en la BD solo queda su hash.
pub async fn rotate_agent_credentials(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let secret = auth::generate_secret();
    let res = sqlx::query("UPDATE servers SET agent_secret_hash = $2, updated_at = NOW() WHERE id = $1")
        .bind(id)
        .bind(auth::hash_secret(&secret))
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Server {id} not found")));
    }
    // Ahora lo monitorea el agente: se descarta el cliente del recolector central
    state.docker_clients.write().await.remove(&id);
    state.agents.disconnect(id, CLOSE_REVOKED, "credenciales rotadas").await;

    Ok(Json(serde_json::json!({
        "server_id": id,
        "secret": secret,
        "token": format!("{id}.{secret}"),
    })))
}
