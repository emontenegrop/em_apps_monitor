use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Container {
    pub id: String,
    pub server_id: Uuid,
    pub short_id: Option<String>,
    pub name: Option<String>,
    pub image: Option<String>,
    pub image_id: Option<String>,
    pub status: Option<String>,
    pub state: Option<String>,
    pub container_created_at: Option<DateTime<Utc>>,
    pub ports: serde_json::Value,
    pub labels: serde_json::Value,
    pub networks: serde_json::Value,
    pub mounts: serde_json::Value,
    pub command: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// Serialized form sent from the Docker API collector
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DockerContainer {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub image: String,
    pub image_id: String,
    pub status: String,
    pub state: String,
    pub created: i64,
    pub command: Option<String>,
    pub ports: serde_json::Value,
    pub labels: serde_json::Value,
    pub networks: Vec<String>,
    pub mounts: serde_json::Value,
}
