use serde::{Deserialize, Serialize};

/// Contenedor tal como lo reporta la API de Docker/Podman
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
