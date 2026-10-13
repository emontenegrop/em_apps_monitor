//! Autenticación de administradores de la UI/API: usuario + contraseña (argon2id)
//! y sesión en cookie `HttpOnly; Secure; SameSite=Strict`.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{
        header::{COOKIE, SET_COOKIE},
        HeaderMap, HeaderValue, StatusCode,
    },
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    agents::auth::{generate_secret, hash_secret},
    error::{ApiResult, AppError},
    state::AppState,
};

pub const SESSION_COOKIE: &str = "em_session";
const SESSION_TTL_HOURS: i64 = 12;
const MIN_PASSWORD_LEN: usize = 12;

/// Usuario autenticado de la petición
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: Uuid,
    pub username: String,
}

// ── Contraseñas ────────────────────────────────────────────────────────────

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("hash de contraseña: {e}"))
}

fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

/// Hash fijo para comparar cuando el usuario no existe, y así no revelar por tiempo
/// de respuesta qué usuarios existen
fn dummy_hash() -> &'static str {
    use std::sync::OnceLock;
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| hash_password("contraseña-que-nunca-coincide").unwrap_or_default())
}

/// Crea el administrador inicial desde ADMIN_USERNAME / ADMIN_PASSWORD si no hay usuarios
pub async fn bootstrap_admin(db: &PgPool, username: Option<&str>, password: Option<&str>) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(db).await?;
    if count > 0 {
        return Ok(());
    }
    let (Some(username), Some(password)) = (username, password) else {
        tracing::error!(
            "no hay administradores: define ADMIN_USERNAME y ADMIN_PASSWORD para crear el primero. \
             Hasta entonces la API rechaza todas las peticiones."
        );
        return Ok(());
    };
    if password.chars().count() < MIN_PASSWORD_LEN {
        anyhow::bail!("ADMIN_PASSWORD debe tener al menos {MIN_PASSWORD_LEN} caracteres");
    }
    sqlx::query("INSERT INTO users (username, password_hash) VALUES ($1, $2)")
        .bind(username)
        .bind(hash_password(password)?)
        .execute(db)
        .await?;
    tracing::warn!("administrador inicial '{username}' creado: cambia la contraseña y quita ADMIN_PASSWORD del entorno");
    Ok(())
}

// ── Límite de intentos ───────────────────────────────────────────────────────

/// Límite de intentos fallidos por IP (login e inscripción de agentes)
pub struct RateLimiter {
    max: u32,
    window: Duration,
    hits: Mutex<HashMap<String, (u32, Instant)>>,
}

impl RateLimiter {
    pub fn new(max: u32, window: Duration) -> Self {
        Self { max, window, hits: Mutex::new(HashMap::new()) }
    }

    /// `false` si la IP superó el límite de fallos en la ventana actual
    pub fn allowed(&self, key: &str) -> bool {
        let mut hits = self.hits.lock().unwrap();
        hits.retain(|_, (_, start)| start.elapsed() < self.window);
        hits.get(key).is_none_or(|(n, _)| *n < self.max)
    }

    pub fn record_failure(&self, key: &str) {
        let mut hits = self.hits.lock().unwrap();
        let entry = hits.entry(key.to_string()).or_insert((0, Instant::now()));
        entry.0 += 1;
    }

    pub fn reset(&self, key: &str) {
        self.hits.lock().unwrap().remove(key);
    }
}

/// IP del cliente: la que informa nginx (`X-Real-IP`) o la del socket
pub fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| peer.map(|p| p.ip().to_string()))
        .unwrap_or_else(|| "desconocida".into())
}

// ── Sesiones ─────────────────────────────────────────────────────────────────

fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == SESSION_COOKIE)
        .map(|(_, v)| v.to_string())
}

fn session_cookie(token: &str, secure: bool, max_age_secs: i64) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age_secs}{secure}"
    ))
    .expect("cookie con caracteres válidos")
}

async fn user_for_session(db: &PgPool, token: &str) -> sqlx::Result<Option<CurrentUser>> {
    let row: Option<(Uuid, String)> = sqlx::query_as(
        r#"SELECT u.id, u.username FROM sessions s JOIN users u ON u.id = s.user_id
           WHERE s.token_hash = $1 AND s.expires_at > NOW()"#,
    )
    .bind(hash_secret(token))
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(id, username)| CurrentUser { id, username }))
}

/// Middleware: exige una sesión válida y deja el usuario en las extensiones de la petición
pub async fn require_session(State(state): State<Arc<AppState>>, mut req: Request, next: Next) -> Response {
    let Some(token) = session_token(req.headers()) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "sesión requerida" }))).into_response();
    };
    match user_for_session(&state.db, &token).await {
        Ok(Some(user)) => {
            req.extensions_mut().insert(user);
            next.run(req).await
        }
        Ok(None) => (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "sesión vencida o inválida" })))
            .into_response(),
        Err(e) => AppError::from(e).into_response(),
    }
}

/// Borra las sesiones vencidas (se llama periódicamente)
pub async fn prune_sessions(db: &PgPool) -> sqlx::Result<u64> {
    Ok(sqlx::query("DELETE FROM sessions WHERE expires_at < NOW()").execute(db).await?.rows_affected())
}

// ── Endpoints ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> ApiResult<Response> {
    let ip = client_ip(&headers, Some(peer));
    if !state.login_limiter.allowed(&ip) {
        tracing::warn!("login bloqueado temporalmente para {ip}");
        return Ok((StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({ "error": "demasiados intentos; espera unos minutos" }))).into_response());
    }

    let row: Option<(Uuid, String)> = sqlx::query_as("SELECT id, password_hash FROM users WHERE username = $1")
        .bind(&body.username)
        .fetch_optional(&state.db)
        .await?;
    let password = body.password.clone();
    let hash = row.as_ref().map(|(_, h)| h.clone()).unwrap_or_else(|| dummy_hash().to_string());
    // argon2 es costoso a propósito: fuera del hilo del runtime
    let ok = tokio::task::spawn_blocking(move || verify_password(&password, &hash))
        .await
        .unwrap_or(false);

    let Some((user_id, _)) = row.filter(|_| ok) else {
        state.login_limiter.record_failure(&ip);
        tracing::warn!("login fallido para '{}' desde {ip}", body.username);
        return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "usuario o contraseña incorrectos" }))).into_response());
    };
    state.login_limiter.reset(&ip);

    let token = generate_secret();
    sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, NOW() + make_interval(hours => $3))")
        .bind(hash_secret(&token))
        .bind(user_id)
        .bind(SESSION_TTL_HOURS as i32)
        .execute(&state.db)
        .await?;
    tracing::info!("login de '{}' desde {ip}", body.username);

    let mut resp = Json(serde_json::json!({ "username": body.username })).into_response();
    resp.headers_mut()
        .insert(SET_COOKIE, session_cookie(&token, state.cookie_secure, SESSION_TTL_HOURS * 3600));
    Ok(resp)
}

pub async fn logout(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(token) = session_token(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(hash_secret(&token))
            .execute(&state.db)
            .await?;
    }
    let mut resp = StatusCode::NO_CONTENT.into_response();
    resp.headers_mut().insert(SET_COOKIE, session_cookie("", state.cookie_secure, 0));
    Ok(resp)
}

pub async fn me(Extension(user): Extension<CurrentUser>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "id": user.id, "username": user.username }))
}

#[derive(Deserialize)]
pub struct ChangePassword {
    pub current_password: String,
    pub new_password: String,
}

/// Cambia la contraseña y cierra las demás sesiones del usuario
pub async fn change_password(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<CurrentUser>,
    headers: HeaderMap,
    Json(body): Json<ChangePassword>,
) -> ApiResult<StatusCode> {
    if body.new_password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::BadRequest(format!("la contraseña debe tener al menos {MIN_PASSWORD_LEN} caracteres")));
    }
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(user.id)
        .fetch_one(&state.db)
        .await?;
    let current = body.current_password.clone();
    if !tokio::task::spawn_blocking(move || verify_password(&current, &hash)).await.unwrap_or(false) {
        return Err(AppError::BadRequest("la contraseña actual no es correcta".into()));
    }
    let new_hash = tokio::task::spawn_blocking(move || hash_password(&body.new_password))
        .await
        .map_err(|e| AppError::Internal(e.into()))??;

    let keep = session_token(&headers).map(|t| hash_secret(&t)).unwrap_or_default();
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE users SET password_hash = $2, updated_at = NOW() WHERE id = $1")
        .bind(user.id)
        .bind(new_hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND token_hash <> $2")
        .bind(user.id)
        .bind(keep)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!("'{}' cambió su contraseña", user.username);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let h = hash_password("una-clave-larga-123").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("una-clave-larga-123", &h));
        assert!(!verify_password("otra", &h));
        assert!(!verify_password("x", "no-es-un-hash"));
    }

    #[test]
    fn reads_session_cookie_among_others() {
        let mut h = HeaderMap::new();
        h.insert(COOKIE, HeaderValue::from_static("tema=oscuro; em_session=abc123; otra=1"));
        assert_eq!(session_token(&h).as_deref(), Some("abc123"));
        assert_eq!(session_token(&HeaderMap::new()), None);
    }

    #[test]
    fn rate_limiter_blocks_after_max_failures() {
        let rl = RateLimiter::new(3, Duration::from_secs(60));
        for _ in 0..3 {
            assert!(rl.allowed("1.2.3.4"));
            rl.record_failure("1.2.3.4");
        }
        assert!(!rl.allowed("1.2.3.4"));
        assert!(rl.allowed("5.6.7.8"));
        rl.reset("1.2.3.4");
        assert!(rl.allowed("1.2.3.4"));
    }
}
