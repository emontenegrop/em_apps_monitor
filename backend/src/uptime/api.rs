//! API de disponibilidad: registro/eliminación de URLs, datos del tablero y configuración
//! de notificaciones. Todas las rutas requieren sesión de administrador.

use std::{net::IpAddr, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    checker::parse_expected_codes,
    model::{CheckRecord, Target},
    notify::{self, SmtpSettings},
    worker::{self, InflightGuard},
};
use crate::{
    error::{ApiResult, AppError},
    state::AppState,
};

const DEFAULT_CODES: &str = "200,201,202,301,302";

fn conflict_or(e: sqlx::Error, msg: &str) -> AppError {
    match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => AppError::Conflict(msg.into()),
        _ => AppError::Database(e),
    }
}

// ── URLs monitoreadas ──────────────────────────────────────────────────────

pub async fn list_targets(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<Target>>> {
    let rows = sqlx::query_as::<_, Target>("SELECT * FROM monitored_targets ORDER BY name")
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct CreateTarget {
    pub name: String,
    pub url: String,
    pub description: Option<String>,
    pub monitor_type: String,
    pub check_interval_secs: Option<i32>,
    pub timeout_secs: Option<i32>,
    pub expected_status_codes: Option<String>,
    pub alert_threshold: Option<i32>,
}

/// Valida una URL http(s) y la devuelve normalizada
fn validate_http_url(raw: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(raw).map_err(|_| "URL inválida (ej. https://servicio.gob.ec/)".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("La URL debe empezar con http:// o https://".into());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err("La URL no tiene host".into());
    }
    Ok(raw.to_string())
}

/// Host o IP para ping (sin esquema ni puerto)
fn validate_ping_host(raw: &str) -> Result<String, String> {
    if raw.parse::<IpAddr>().is_ok() {
        return Ok(raw.to_string());
    }
    let looks_like_ip = raw.chars().all(|c| c.is_ascii_digit() || c == '.');
    let valid_name = !raw.is_empty()
        && raw.len() <= 253
        && raw.split('.').all(|l| {
            !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-')
                && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        });
    if looks_like_ip || !valid_name {
        return Err("Host o IP inválido (ej. 172.16.27.31 o servidor.local)".into());
    }
    Ok(raw.to_string())
}

fn validate_codes(raw: &str) -> Result<String, String> {
    let tokens: Vec<&str> = raw.split(',').map(str::trim).filter(|t| !t.is_empty()).collect();
    let codes = parse_expected_codes(raw);
    if tokens.is_empty() || codes.len() != tokens.len() || codes.iter().any(|c| !(100..=599).contains(c)) {
        return Err("Códigos HTTP esperados inválidos (ej. 200,301,302)".into());
    }
    Ok(codes.iter().map(u16::to_string).collect::<Vec<_>>().join(","))
}

fn in_range(v: Option<i32>, default: i32, min: i32, max: i32, field: &str) -> Result<i32, String> {
    let v = v.unwrap_or(default);
    if (min..=max).contains(&v) {
        Ok(v)
    } else {
        Err(format!("{field} debe estar entre {min} y {max}"))
    }
}

pub async fn create_target(
    State(state): State<Arc<AppState>>,
    Json(p): Json<CreateTarget>,
) -> ApiResult<Json<Target>> {
    let bad = AppError::BadRequest;
    let name = p.name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err(bad("El nombre es obligatorio (máx. 255 caracteres)".into()));
    }
    let raw = p.url.trim();
    if raw.len() > 512 {
        return Err(bad("La URL o host no puede superar 512 caracteres".into()));
    }
    let (url, codes) = match p.monitor_type.as_str() {
        "http" => (
            validate_http_url(raw).map_err(bad)?,
            validate_codes(p.expected_status_codes.as_deref().unwrap_or(DEFAULT_CODES)).map_err(bad)?,
        ),
        "ping" => (validate_ping_host(raw).map_err(bad)?, DEFAULT_CODES.to_string()),
        _ => return Err(bad("El tipo debe ser http o ping".into())),
    };
    let interval = in_range(p.check_interval_secs, 60, 30, 3600, "El intervalo").map_err(bad)?;
    let timeout = in_range(p.timeout_secs, 10, 1, 60, "El timeout").map_err(bad)?;
    let threshold = in_range(p.alert_threshold, 1, 1, 20, "El umbral de alerta").map_err(bad)?;
    let description = p.description.as_deref().map(str::trim).filter(|d| !d.is_empty());

    let target = sqlx::query_as::<_, Target>(
        r#"INSERT INTO monitored_targets
           (name, url, description, monitor_type, check_interval_secs, timeout_secs,
            expected_status_codes, alert_threshold)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING *"#,
    )
    .bind(name)
    .bind(&url)
    .bind(description)
    .bind(&p.monitor_type)
    .bind(interval)
    .bind(timeout)
    .bind(&codes)
    .bind(threshold)
    .fetch_one(&state.db)
    .await
    .map_err(|e| conflict_or(e, "Esa URL o host ya está registrado"))?;

    tracing::info!("disponibilidad: registrado {} ({})", target.name, target.url);
    Ok(Json(target))
}

pub async fn delete_target(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> ApiResult<Json<serde_json::Value>> {
    let name: Option<String> = sqlx::query_scalar("DELETE FROM monitored_targets WHERE id = $1 RETURNING name")
        .bind(id)
        .fetch_optional(&state.db)
        .await?;
    let name = name.ok_or_else(|| AppError::NotFound("URL no encontrada".into()))?;
    tracing::info!("disponibilidad: eliminado {name} y su historial");
    Ok(Json(serde_json::json!({ "deleted": id })))
}

#[derive(Deserialize)]
pub struct SetActive {
    pub is_active: bool,
}

pub async fn set_target_active(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(p): Json<SetActive>,
) -> ApiResult<Json<Target>> {
    // Al pausar se limpia el estado: al reactivar empieza de cero (sin alertas por fallos viejos)
    let target = sqlx::query_as::<_, Target>(
        r#"UPDATE monitored_targets
           SET is_active = $2, updated_at = NOW(),
               last_status = CASE WHEN $2 THEN last_status ELSE NULL END,
               consecutive_failures = CASE WHEN $2 THEN consecutive_failures ELSE 0 END,
               notified_status = CASE WHEN $2 THEN notified_status ELSE NULL END
           WHERE id = $1 RETURNING *"#,
    )
    .bind(id)
    .bind(p.is_active)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("URL no encontrada".into()))?;
    Ok(Json(target))
}

/// Verifica la URL en este momento (además de las verificaciones periódicas)
pub async fn check_target_now(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> ApiResult<Json<CheckRecord>> {
    let target = sqlx::query_as::<_, Target>("SELECT * FROM monitored_targets WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("URL no encontrada".into()))?;
    let _guard = InflightGuard::acquire(&state, id)
        .ok_or_else(|| AppError::Conflict("Ya hay una verificación en curso para esta URL".into()))?;
    worker::run_check(&state, &target)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::NotFound("La URL se eliminó durante la verificación".into()))
}

// ── Tablero ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TzQuery {
    /// Zona horaria IANA para agrupar por día (la del navegador)
    pub tz: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct DailyAvailability {
    pub target_id: Uuid,
    pub day: NaiveDate,
    pub total: i64,
    pub success: i64,
}

/// Disponibilidad diaria de los últimos 7 días (hoy incluido)
pub async fn availability(
    State(state): State<Arc<AppState>>,
    Query(q): Query<TzQuery>,
) -> ApiResult<Json<Vec<DailyAvailability>>> {
    let tz = q.tz.unwrap_or_else(|| "America/Guayaquil".into());
    let known: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = $1)")
        .bind(&tz)
        .fetch_one(&state.db)
        .await?;
    if !known {
        return Err(AppError::BadRequest(format!("Zona horaria desconocida: {tz}")));
    }
    let rows = sqlx::query_as::<_, DailyAvailability>(
        r#"SELECT target_id, (time AT TIME ZONE $1)::date AS day,
                  count(*) AS total, count(*) FILTER (WHERE is_success) AS success
             FROM target_checks
            WHERE time >= (date_trunc('day', NOW() AT TIME ZONE $1) - INTERVAL '6 days') AT TIME ZONE $1
            GROUP BY 1, 2
            ORDER BY 2"#,
    )
    .bind(&tz)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct AvgResponse {
    pub target_id: Uuid,
    pub avg_ms: Option<f64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ResponsePoint {
    pub target_id: Uuid,
    pub time: DateTime<Utc>,
    pub response_ms: Option<f64>,
}

#[derive(Serialize)]
pub struct ResponseTimes {
    /// Promedio de las verificaciones exitosas de las últimas 24 h
    pub avg_24h: Vec<AvgResponse>,
    /// Verificaciones de la última hora
    pub last_hour: Vec<ResponsePoint>,
}

pub async fn response_times(State(state): State<Arc<AppState>>) -> ApiResult<Json<ResponseTimes>> {
    let avg_24h = sqlx::query_as::<_, AvgResponse>(
        r#"SELECT target_id, avg(response_ms) AS avg_ms FROM target_checks
            WHERE is_success AND time >= NOW() - INTERVAL '24 hours' GROUP BY 1"#,
    )
    .fetch_all(&state.db)
    .await?;
    let last_hour = sqlx::query_as::<_, ResponsePoint>(
        r#"SELECT target_id, time, response_ms FROM target_checks
            WHERE time >= NOW() - INTERVAL '1 hour' ORDER BY time"#,
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ResponseTimes { avg_24h, last_hour }))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Incident {
    pub target_id: Uuid,
    pub time: DateTime<Utc>,
    pub status_code: Option<i32>,
    pub error_message: Option<String>,
}

/// Verificaciones fallidas de los últimos 7 días
pub async fn incidents(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<Incident>>> {
    let rows = sqlx::query_as::<_, Incident>(
        r#"SELECT target_id, time, status_code, error_message FROM target_checks
            WHERE NOT is_success AND time >= NOW() - INTERVAL '7 days'
            ORDER BY time DESC LIMIT 5000"#,
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

// ── Notificaciones ─────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct Recipient {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct NotificationConfig {
    /// `None` hasta que se guarde por primera vez
    pub settings: Option<SmtpSettings>,
    pub password_set: bool,
    /// El backend puede cifrar la contraseña (tiene SETTINGS_KEY)
    pub can_store_password: bool,
    pub recipients: Vec<Recipient>,
}

pub async fn get_notifications(State(state): State<Arc<AppState>>) -> ApiResult<Json<NotificationConfig>> {
    let settings = notify::load_settings(&state.db).await?;
    let recipients = sqlx::query_as::<_, Recipient>("SELECT * FROM notification_recipients ORDER BY email")
        .fetch_all(&state.db)
        .await?;
    Ok(Json(NotificationConfig {
        password_set: settings.as_ref().is_some_and(|s| s.password_enc.is_some()),
        settings,
        can_store_password: state.settings_cipher.is_some(),
        recipients,
    }))
}

#[derive(Deserialize)]
pub struct SaveSettings {
    pub enabled: bool,
    pub smtp_host: String,
    pub smtp_port: i32,
    pub security: String,
    pub username: String,
    /// Ausente o vacía: se conserva la guardada
    pub password: Option<String>,
    pub from_address: String,
    pub single_email_bcc: bool,
}

fn valid_email(s: &str) -> bool {
    s.parse::<lettre::Address>().is_ok()
}

pub async fn save_notifications(
    State(state): State<Arc<AppState>>,
    Json(p): Json<SaveSettings>,
) -> ApiResult<Json<NotificationConfig>> {
    let bad = |m: &str| AppError::BadRequest(m.into());
    let host = p.smtp_host.trim();
    if host.is_empty() || host.len() > 255 {
        return Err(bad("El servidor SMTP es obligatorio"));
    }
    if !(1..=65535).contains(&p.smtp_port) {
        return Err(bad("Puerto SMTP inválido"));
    }
    if !matches!(p.security.as_str(), "starttls" | "ssl" | "none") {
        return Err(bad("La seguridad debe ser starttls, ssl o none"));
    }
    let from = p.from_address.trim();
    if !valid_email(from) {
        return Err(bad("El remitente no es un correo válido"));
    }
    let username = p.username.trim();
    let password_enc = match p.password.as_deref().filter(|pw| !pw.is_empty()) {
        Some(pw) => Some(
            state
                .settings_cipher
                .as_ref()
                .ok_or_else(|| bad("El backend no tiene SETTINGS_KEY: no se puede guardar la contraseña"))?
                .encrypt(pw)?,
        ),
        None => None,
    };

    sqlx::query(
        r#"INSERT INTO notification_settings
           (id, enabled, smtp_host, smtp_port, security, username, password_enc, from_address, single_email_bcc, updated_at)
           VALUES (1, $1, $2, $3, $4, $5, $6, $7, $8, NOW())
           ON CONFLICT (id) DO UPDATE SET
             enabled = EXCLUDED.enabled, smtp_host = EXCLUDED.smtp_host, smtp_port = EXCLUDED.smtp_port,
             security = EXCLUDED.security, username = EXCLUDED.username,
             password_enc = COALESCE(EXCLUDED.password_enc, notification_settings.password_enc),
             from_address = EXCLUDED.from_address, single_email_bcc = EXCLUDED.single_email_bcc,
             updated_at = NOW()"#,
    )
    .bind(p.enabled)
    .bind(host)
    .bind(p.smtp_port)
    .bind(&p.security)
    .bind(username)
    .bind(password_enc)
    .bind(from)
    .bind(p.single_email_bcc)
    .execute(&state.db)
    .await?;

    tracing::info!("notificaciones: configuración SMTP actualizada ({host}:{})", p.smtp_port);
    get_notifications(State(state)).await
}

#[derive(Deserialize)]
pub struct AddRecipient {
    pub email: String,
    pub name: Option<String>,
}

pub async fn add_recipient(
    State(state): State<Arc<AppState>>,
    Json(p): Json<AddRecipient>,
) -> ApiResult<Json<Recipient>> {
    let email = p.email.trim();
    if email.len() > 255 || !valid_email(email) {
        return Err(AppError::BadRequest("Correo inválido".into()));
    }
    let name = p.name.as_deref().map(str::trim).filter(|n| !n.is_empty());
    let r = sqlx::query_as::<_, Recipient>(
        "INSERT INTO notification_recipients (email, name) VALUES ($1, $2) RETURNING *",
    )
    .bind(email)
    .bind(name)
    .fetch_one(&state.db)
    .await
    .map_err(|e| conflict_or(e, "Ese correo ya está registrado"))?;
    Ok(Json(r))
}

pub async fn delete_recipient(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> ApiResult<Json<serde_json::Value>> {
    let done = sqlx::query("DELETE FROM notification_recipients WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("Destinatario no encontrado".into()));
    }
    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn test_notifications(State(state): State<Arc<AppState>>) -> ApiResult<Json<serde_json::Value>> {
    match notify::send_test(&state).await {
        Ok(sent) => Ok(Json(serde_json::json!({ "sent": sent }))),
        // El detalle del error SMTP le sirve al administrador para corregir la configuración
        Err(e) => Err(AppError::BadRequest(format!("No se pudo enviar: {e:#}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_urls() {
        assert!(validate_http_url("https://example.com/").is_ok());
        assert!(validate_http_url("ftp://x").is_err());
        assert!(validate_http_url("example.com").is_err());
    }

    #[test]
    fn ping_hosts() {
        assert!(validate_ping_host("172.16.27.31").is_ok());
        assert!(validate_ping_host("servidor.local").is_ok());
        assert!(validate_ping_host("192.16.27.261").is_err());
        assert!(validate_ping_host("https://x").is_err());
        assert!(validate_ping_host("-malo.local").is_err());
    }

    #[test]
    fn codes() {
        assert_eq!(validate_codes(" 200, 301 ").unwrap(), "200,301");
        assert!(validate_codes("200,abc").is_err());
        assert!(validate_codes("999").is_err());
        assert!(validate_codes("").is_err());
    }
}
