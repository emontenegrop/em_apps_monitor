use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Server {
    pub id: Uuid,
    pub name: String,
    pub tags: serde_json::Value,
    pub is_active: bool,
    pub status: String, // online | offline | unknown
    pub last_seen_at: Option<DateTime<Utc>>,
    // ── Agente (agent_secret_hash y enrollment_token_hash nunca se exponen) ──
    /// Tiene una credencial vigente (inscrito y no revocado)
    pub agent_enrolled: bool,
    /// Vencimiento de la ficha de inscripción pendiente, si hay una
    pub enrollment_expires_at: Option<DateTime<Utc>>,
    pub agent_version: Option<String>,
    pub agent_protocol_version: Option<i32>,
    pub hostname: Option<String>,
    pub os: Option<String>,
    pub arch: Option<String>,
    /// [{kind, socket, version, rootless, error}]
    pub runtimes: serde_json::Value,
    pub last_connected_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateServer {
    pub name: String,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateServer {
    pub name: Option<String>,
    pub tags: Option<Vec<String>>,
    pub is_active: Option<bool>,
}
