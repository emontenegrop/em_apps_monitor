use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use em_protocol::metrics::{ContainerMetrics, DiskUsage, DockerInfo, HostMetrics};

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
        data: DockerInfo,
    },
    ServerStatus {
        server_id: Uuid,
        status: String,
    },
}
