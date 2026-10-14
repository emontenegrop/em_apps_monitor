//! Mensajes del WebSocket agente ↔ central (JSON, etiquetados por `type`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    container::DockerContainer,
    metrics::{ContainerMetrics, DiskUsage, DockerInfo, HostMetrics},
};

/// Motor de contenedores detectado en el host
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Docker,
    Podman,
}

/// Endpoint de runtime que el agente monitorea
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuntimeDescriptor {
    pub kind: RuntimeKind,
    pub socket: String,
    pub version: Option<String>,
    pub rootless: bool,
}

/// Estado de un runtime en un ciclo de recolección
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuntimeSnapshot {
    pub kind: RuntimeKind,
    pub socket: String,
    /// El runtime corre sin root. Va en cada snapshot porque el agente puede detectar
    /// runtimes después del Hello (p. ej. Docker o Podman arrancaron más tarde).
    #[serde(default)]
    pub rootless: bool,
    /// `None` si el runtime no respondió en este ciclo (ver `error`)
    pub info: Option<DockerInfo>,
    pub error: Option<String>,
    /// `None` cuando la lista no cambió desde el snapshot anterior
    pub containers: Option<Vec<DockerContainer>>,
    pub stats: Vec<ContainerMetrics>,
}

/// Foto periódica de un host
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Snapshot {
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub host: HostMetrics,
    pub disks: Vec<DiskUsage>,
    pub runtimes: Vec<RuntimeSnapshot>,
}

/// Operaciones de solo lectura que el central puede pedir al agente
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum RequestMethod {
    Ping,
    Disks,
    Inspect {
        runtime: RuntimeKind,
        container_id: String,
        /// Por defecto los valores de `Config.Env` se ocultan antes de salir del host
        #[serde(default)]
        reveal_env: bool,
    },
    Logs { runtime: RuntimeKind, container_id: String, tail: u32 },
    Images { runtime: RuntimeKind },
    Networks { runtime: RuntimeKind },
    Volumes { runtime: RuntimeKind },
}

/// Agente → central
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentMessage {
    Hello {
        protocol_version: u32,
        agent_version: String,
        hostname: String,
        os: Option<String>,
        arch: String,
        kernel: Option<String>,
        runtimes: Vec<RuntimeDescriptor>,
    },
    Snapshot(Snapshot),
    /// Snapshots acumulados mientras no había conexión
    Backfill { snapshots: Vec<Snapshot> },
    Response {
        req_id: Uuid,
        #[serde(flatten)]
        result: RequestResult,
    },
    LogChunk {
        req_id: Uuid,
        lines: Vec<String>,
        eof: bool,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum RequestResult {
    Ok(serde_json::Value),
    Error(String),
}

/// Central → agente
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CentralMessage {
    Welcome {
        server_id: Uuid,
        collection_interval_secs: u64,
        min_protocol_version: u32,
    },
    Request {
        req_id: Uuid,
        #[serde(flatten)]
        method: RequestMethod,
    },
    ConfigUpdate {
        collection_interval_secs: Option<u64>,
        log_level: Option<String>,
    },
}

/// Código de cierre cuando una conexión nueva del mismo agente reemplaza a esta
pub const CLOSE_REPLACED: u16 = 4000;
/// Código de cierre del WebSocket cuando la versión del protocolo no es compatible
pub const CLOSE_INCOMPATIBLE_PROTOCOL: u16 = 4001;
/// Código de cierre cuando las credenciales del agente fueron revocadas
pub const CLOSE_REVOKED: u16 = 4003;
/// Código de cierre cuando la credencial ya está en uso por un agente en otro host
pub const CLOSE_DUPLICATE: u16 = 4009;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip() {
        let msg = CentralMessage::Request {
            req_id: Uuid::nil(),
            method: RequestMethod::Logs {
                runtime: RuntimeKind::Podman,
                container_id: "abc".into(),
                tail: 200,
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(
            json,
            r#"{"type":"request","req_id":"00000000-0000-0000-0000-000000000000","method":"logs","runtime":"podman","container_id":"abc","tail":200}"#
        );
        let back: CentralMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            back,
            CentralMessage::Request { method: RequestMethod::Logs { tail: 200, .. }, .. }
        ));
    }

    #[test]
    fn response_roundtrip() {
        let msg = AgentMessage::Response {
            req_id: Uuid::nil(),
            result: RequestResult::Error("offline".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(
            json,
            r#"{"type":"response","req_id":"00000000-0000-0000-0000-000000000000","error":"offline"}"#
        );
        let back: AgentMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            back,
            AgentMessage::Response { result: RequestResult::Error(_), .. }
        ));
    }
}
