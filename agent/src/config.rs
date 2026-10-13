use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use em_protocol::messages::RuntimeKind;
use serde::Deserialize;

pub const DEFAULT_CONFIG_PATH: &str = "/etc/em-agent/em-agent.toml";

/// Configuración del agente (`/etc/em-agent/em-agent.toml`)
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentConfig {
    /// URL del central, p. ej. `https://172.16.27.31:8443`
    pub server_url: Option<String>,
    /// CA interna con la que se valida el certificado del central
    pub ca_file: PathBuf,
    pub collection_interval_secs: u64,
    /// Sockets a monitorear. Vacío = detección automática.
    pub runtimes: Vec<RuntimeEndpoint>,
    /// Raíz del host montada dentro del contenedor (modo contenedor), p. ej. `/host`
    pub host_root: Option<PathBuf>,
    /// Stats de contenedores pedidas en paralelo
    pub stats_concurrency: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeEndpoint {
    /// `unix:///run/podman/podman.sock` o una ruta directa
    pub socket: String,
    /// Forzar el tipo; si se omite se detecta con `/version`
    pub kind: Option<RuntimeKind>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            server_url: None,
            ca_file: PathBuf::from("/etc/em-agent/ca.crt"),
            collection_interval_secs: 15,
            runtimes: Vec::new(),
            host_root: None,
            stats_concurrency: 8,
        }
    }
}

impl AgentConfig {
    /// Carga el archivo si existe; si es la ruta por defecto y no existe, usa valores por defecto.
    pub fn load(path: &Path, explicit: bool) -> Result<Self> {
        let mut cfg = if path.exists() {
            let raw = std::fs::read_to_string(path)
                .with_context(|| format!("no se pudo leer {}", path.display()))?;
            toml::from_str(&raw).with_context(|| format!("configuración inválida en {}", path.display()))?
        } else if explicit {
            anyhow::bail!("no existe el archivo de configuración {}", path.display());
        } else {
            Self::default()
        };

        if let Ok(root) = std::env::var("EM_AGENT_HOST_ROOT") {
            cfg.host_root = Some(PathBuf::from(root));
        }
        if let Ok(url) = std::env::var("EM_AGENT_SERVER_URL") {
            cfg.server_url = Some(url);
        }
        cfg.stats_concurrency = cfg.stats_concurrency.max(1);
        cfg.collection_interval_secs = cfg.collection_interval_secs.max(5);
        Ok(cfg)
    }
}
