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

pub use em_protocol::container::DockerContainer;
