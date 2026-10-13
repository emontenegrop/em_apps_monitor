use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use em_protocol::metrics::{ContainerMetrics, DiskUsage, DockerInfo, HostMetrics};

/// Info de un runtime de un servidor (un host puede tener Docker y Podman)
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RuntimeInfo {
    pub runtime_kind: String,
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub info: DockerInfo,
}

/// Realtime snapshot broadcast via WebSocket
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    HostMetrics {
        server_id: Uuid,
        data: HostMetrics,
    },
    ContainerMetrics {
        server_id: Uuid,
        data: Vec<ContainerMetrics>,
    },
    Containers {
        server_id: Uuid,
        data: Vec<crate::models::container::DockerContainer>,
    },
    DockerInfo {
        server_id: Uuid,
        /// docker | podman (None en el recolector central, que solo habla con Docker)
        #[serde(skip_serializing_if = "Option::is_none")]
        runtime: Option<String>,
        data: DockerInfo,
    },
    ServerStatus {
        server_id: Uuid,
        status: String,
    },
}
