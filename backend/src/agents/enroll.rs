//! Inscripción de agentes con fichas de un solo uso, revocación y publicación de la CA.

use std::{net::SocketAddr, path::Path as FsPath, sync::Arc};

use axum::{
    extract::{ConnectInfo, Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use base64::Engine;
use chrono::{DateTime, Utc};
use em_protocol::messages::CLOSE_REVOKED;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    auth::{generate_secret, hash_secret},
    ingest::broadcast,
};
use crate::{
    auth::{client_ip, CurrentUser},
    error::{ApiResult, AppError},
    models::metrics::WsEvent,
    state::AppState,
};

/// Validez de una ficha de inscripción
const ENROLLMENT_TTL_MINUTES: i32 = 60;

/// CA interna que se entrega a los agentes, con su huella SHA-256 (del DER, en hex)
#[derive(Clone)]
pub struct AgentCa {
    pub pem: String,
    pub fingerprint: String,
}

impl AgentCa {
    pub fn load(path: &FsPath) -> anyhow::Result<Self> {
        let pem = std::fs::read_to_string(path)?;
        Ok(Self { fingerprint: pem_fingerprint(&pem)?, pem })
    }
}

/// Huella SHA-256 del primer certificado de un PEM
pub fn pem_fingerprint(pem: &str) -> anyhow::Result<String> {
    let b64: String = pem
        .lines()
        .skip_while(|l| !l.starts_with("-----BEGIN CERTIFICATE-----"))
        .skip(1)
        .take_while(|l| !l.starts_with("-----END CERTIFICATE-----"))
        .collect();
    if b64.is_empty() {
        anyhow::bail!("el PEM no contiene un certificado");
    }
    let der = base64::engine::general_purpose::STANDARD.decode(b64.trim())?;
    Ok(hex::encode(Sha256::digest(der)))
}

#[derive(Serialize)]
pub struct EnrollmentToken {
    pub server_id: Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub server_url: String,
    pub ca_fingerprint: Option<String>,
    pub install_command: Option<String>,
}

/// URL pública del central: PUBLIC_URL o, si no está definida, la del Host de la petición
fn public_url(state: &AppState, headers: &HeaderMap) -> String {
    state.public_url.clone().unwrap_or_else(|| {
        let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("localhost");
        format!("https://{host}")
    })
}

/// `POST /api/servers/:id/enrollment-token` (administrador): genera una ficha de un
/// solo uso. Invalida la ficha anterior; la credencial vigente del agente sigue valiendo
/// hasta que se use la nueva ficha.
pub async fn create_enrollment_token(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Json<EnrollmentToken>> {
    let token = generate_secret();
    let expires_at: Option<DateTime<Utc>> = sqlx::query_scalar(
        r#"UPDATE servers SET uses_agent = true, enrollment_token_hash = $2,
             enrollment_expires_at = NOW() + make_interval(mins => $3), updated_at = NOW()
           WHERE id = $1 RETURNING enrollment_expires_at"#,
    )
    .bind(id)
    .bind(hash_secret(&token))
    .bind(ENROLLMENT_TTL_MINUTES)
    .fetch_optional(&state.db)
    .await?;
    let expires_at = expires_at.ok_or_else(|| AppError::NotFound(format!("Server {id} not found")))?;
    // Ahora lo monitorea un agente: se descarta el cliente del recolector central
    state.docker_clients.write().await.remove(&id);

    let server_url = public_url(&state, &headers);
    let ca_fingerprint = state.agent_ca.as_ref().map(|ca| ca.fingerprint.clone());
    let install_command = ca_fingerprint.as_ref().map(|fp| {
        format!("sudo em-agent enroll --url {server_url} --token {token} --ca-fingerprint {fp}")
    });
    if ca_fingerprint.is_none() {
        tracing::warn!("sin AGENT_CA_FILE: el comando de instalación no puede incluir la huella de la CA");
    }
    tracing::info!("'{}' generó una ficha de inscripción para el servidor {id}", user.username);

    Ok(Json(EnrollmentToken { server_id: id, token, expires_at, server_url, ca_fingerprint, install_command }))
}

/// `POST /api/servers/:id/revoke` (administrador): invalida la credencial y la ficha
/// del agente y lo desconecta. El servidor queda esperando una nueva inscripción.
pub async fn revoke_agent(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let res = sqlx::query(
        r#"UPDATE servers SET agent_secret_hash = NULL, enrollment_token_hash = NULL,
             enrollment_expires_at = NULL, status = 'offline', updated_at = NOW()
           WHERE id = $1 AND uses_agent"#,
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Server {id} not found or not monitored by an agent")));
    }
    state.agents.disconnect(id, CLOSE_REVOKED, "credencial revocada").await;
    broadcast(&state, &WsEvent::ServerStatus { server_id: id, status: "offline".into() });
    tracing::warn!("'{}' revocó la credencial del agente del servidor {id}", user.username);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct EnrollRequest {
    pub token: String,
    pub hostname: Option<String>,
}

#[derive(Serialize)]
pub struct EnrollResponse {
    pub server_id: Uuid,
    pub secret: String,
}

/// `POST /api/agent/enroll` (público, con límite de intentos): canjea la ficha por la
/// credencial permanente del agente. La ficha se consume en la misma sentencia.
pub async fn enroll(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<EnrollRequest>,
) -> ApiResult<Response> {
    let ip = client_ip(&headers, Some(peer));
    if !state.enroll_limiter.allowed(&ip) {
        return Ok((StatusCode::TOO_MANY_REQUESTS, "demasiados intentos; espera unos minutos").into_response());
    }

    let secret = generate_secret();
    let server_id: Option<Uuid> = sqlx::query_scalar(
        r#"UPDATE servers SET agent_secret_hash = $2, enrollment_token_hash = NULL,
             enrollment_expires_at = NULL, uses_agent = true, updated_at = NOW()
           WHERE enrollment_token_hash = $1 AND enrollment_expires_at > NOW()
           RETURNING id"#,
    )
    .bind(hash_secret(body.token.trim()))
    .bind(hash_secret(&secret))
    .fetch_optional(&state.db)
    .await?;

    let Some(server_id) = server_id else {
        state.enroll_limiter.record_failure(&ip);
        tracing::warn!("inscripción rechazada desde {ip}: ficha inválida, usada o vencida");
        return Ok((StatusCode::UNAUTHORIZED, "ficha inválida, ya usada o vencida").into_response());
    };

    // Si el servidor tenía un agente conectado (reinstalación), su credencial ya no vale
    state.agents.disconnect(server_id, CLOSE_REVOKED, "reemplazado por una nueva inscripción").await;
    tracing::info!(
        "agente inscrito para el servidor {server_id} desde {ip} ({})",
        body.hostname.as_deref().unwrap_or("hostname desconocido")
    );
    Ok(Json(EnrollResponse { server_id, secret }).into_response())
}

/// `GET /api/agent/ca.crt` (público): la CA interna, para que el agente la verifique
/// contra la huella del comando de instalación
pub async fn ca_certificate(State(state): State<Arc<AppState>>) -> Response {
    match &state.agent_ca {
        Some(ca) => ([(header::CONTENT_TYPE, "application/x-pem-file")], ca.pem.clone()).into_response(),
        None => (StatusCode::NOT_FOUND, "el central no tiene AGENT_CA_FILE configurado").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_matches_openssl() {
        // Certificado de prueba y su huella según `openssl x509 -noout -fingerprint -sha256`
        let pem = include_str!("../../tests/fixtures/test-ca.crt");
        let expected = include_str!("../../tests/fixtures/test-ca.sha256").trim();
        assert_eq!(pem_fingerprint(pem).unwrap(), expected);
        assert!(pem_fingerprint("no es un pem").is_err());
    }
}
