use std::{collections::HashSet, path::PathBuf, sync::Arc};

use anyhow::{anyhow, Context, Result};
use bollard::{container::ListContainersOptions, Docker};
use em_protocol::{
    container::DockerContainer,
    messages::{RuntimeDescriptor, RuntimeKind},
    metrics::DockerInfo,
};
use uuid::Uuid;

use crate::config::RuntimeEndpoint;

/// Conexión a un motor de contenedores (Docker o Podman vía su API compatible)
pub struct RuntimeClient {
    pub kind: RuntimeKind,
    pub socket: String,
    pub version: Option<String>,
    pub rootless: bool,
    pub docker: Arc<Docker>,
}

impl RuntimeClient {
    pub fn descriptor(&self) -> RuntimeDescriptor {
        RuntimeDescriptor {
            kind: self.kind,
            socket: self.socket.clone(),
            version: self.version.clone(),
            rootless: self.rootless,
        }
    }

    /// Conecta y detecta el motor con `/version` e `/info`
    pub async fn connect(endpoint: &RuntimeEndpoint) -> Result<Self> {
        let path = endpoint.socket.strip_prefix("unix://").unwrap_or(&endpoint.socket);
        let docker = Docker::connect_with_socket(path, 10, bollard::API_DEFAULT_VERSION)
            .with_context(|| format!("no se pudo abrir {path}"))?;

        let ver = docker
            .version()
            .await
            .with_context(|| format!("{path} no responde a /version"))?;
        let detected = if ver
            .components
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|c| c.name.to_ascii_lowercase().contains("podman"))
        {
            RuntimeKind::Podman
        } else {
            RuntimeKind::Docker
        };
        let kind = endpoint.kind.unwrap_or(detected);
        if endpoint.kind.is_some_and(|k| k != detected) {
            tracing::warn!("{path}: configurado como {kind:?} pero responde como {detected:?}");
        }

        let rootless = docker
            .info()
            .await
            .ok()
            .and_then(|i| i.security_options)
            .is_some_and(|opts| opts.iter().any(|o| o.contains("name=rootless")))
            || path.starts_with("/run/user/");

        Ok(Self {
            kind,
            socket: path.to_string(),
            version: ver.version,
            rootless,
            docker: Arc::new(docker),
        })
    }

    pub async fn info(&self) -> Result<DockerInfo> {
        let info = self.docker.info().await?;
        let ver = self.docker.version().await?;

        let swarm = info
            .swarm
            .as_ref()
            .and_then(|s| s.local_node_state.as_ref())
            .map(|s| matches!(s, bollard::models::LocalNodeState::ACTIVE))
            .unwrap_or(false);

        Ok(DockerInfo {
            server_id: Uuid::nil(),
            docker_version: ver.version.clone(),
            api_version: ver.api_version.clone(),
            os_type: info.os_type.clone(),
            arch: info.architecture.clone(),
            kernel_version: info.kernel_version.clone(),
            containers_total: info.containers.map(|v| v as i32),
            containers_running: info.containers_running.map(|v| v as i32),
            containers_paused: info.containers_paused.map(|v| v as i32),
            containers_stopped: info.containers_stopped.map(|v| v as i32),
            images_total: info.images.map(|v| v as i32),
            storage_driver: info.driver.clone(),
            runtime: ver.platform.as_ref().map(|p| p.name.clone()),
            docker_root_dir: info.docker_root_dir.clone(),
            swarm_active: Some(swarm),
            raw_info: Some(serde_json::to_value(&info)?),
            updated_at: chrono::Utc::now(),
        })
    }

    /// Todos los contenedores (en ejecución y detenidos)
    pub async fn list_containers(&self) -> Result<Vec<DockerContainer>> {
        let raw = self
            .docker
            .list_containers(Some(ListContainersOptions::<String> {
                all: true,
                ..Default::default()
            }))
            .await?;

        Ok(raw
            .into_iter()
            .map(|c| {
                let id = c.id.clone().unwrap_or_default();
                let short_id = id.chars().take(12).collect();
                let name = c
                    .names
                    .as_deref()
                    .and_then(|n| n.first())
                    .map(|n| n.trim_start_matches('/').to_string())
                    .unwrap_or_default();
                let networks = c
                    .network_settings
                    .as_ref()
                    .and_then(|ns| ns.networks.as_ref())
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();

                DockerContainer {
                    id,
                    short_id,
                    name,
                    image: c.image.clone().unwrap_or_default(),
                    image_id: c.image_id.clone().unwrap_or_default(),
                    status: c.status.clone().unwrap_or_default(),
                    state: c.state.clone().unwrap_or_default(),
                    created: c.created.unwrap_or(0),
                    command: c.command.clone(),
                    ports: serde_json::to_value(&c.ports).unwrap_or(serde_json::json!([])),
                    labels: serde_json::to_value(&c.labels).unwrap_or(serde_json::json!({})),
                    networks,
                    mounts: serde_json::to_value(&c.mounts).unwrap_or(serde_json::json!([])),
                }
            })
            .collect())
    }
}

/// Sockets conocidos que se prueban cuando la configuración no define `runtimes`
fn default_sockets() -> Vec<String> {
    let mut sockets = Vec::new();
    if let Ok(host) = std::env::var("DOCKER_HOST") {
        if let Some(path) = host.strip_prefix("unix://") {
            sockets.push(path.to_string());
        }
    }
    sockets.push("/var/run/docker.sock".into());
    sockets.push("/run/podman/podman.sock".into());
    #[cfg(unix)]
    {
        // SAFETY: getuid no tiene precondiciones
        let uid = unsafe { libc::getuid() };
        if uid != 0 {
            sockets.push(format!("/run/user/{uid}/podman/podman.sock"));
            sockets.push(format!("/run/user/{uid}/docker.sock"));
        }
    }
    sockets
}

/// Conecta a los runtimes configurados o, si no hay ninguno, a los que se detecten.
/// Devuelve los clientes y los errores de los sockets que fallaron.
pub async fn discover(configured: &[RuntimeEndpoint]) -> (Vec<RuntimeClient>, Vec<(String, anyhow::Error)>) {
    let auto = configured.is_empty();
    let endpoints: Vec<RuntimeEndpoint> = if auto {
        default_sockets()
            .into_iter()
            .filter(|s| PathBuf::from(s).exists())
            .map(|socket| RuntimeEndpoint { socket, kind: None })
            .collect()
    } else {
        configured.to_vec()
    };

    let mut clients = Vec::new();
    let mut errors = Vec::new();
    // /var/run/docker.sock puede ser un enlace al socket de Podman (paquete podman-docker)
    let mut seen = HashSet::new();
    for ep in &endpoints {
        let path = ep.socket.strip_prefix("unix://").unwrap_or(&ep.socket);
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
        if !seen.insert(canonical) {
            continue;
        }
        match RuntimeClient::connect(ep).await {
            Ok(c) => clients.push(c),
            Err(e) => errors.push((ep.socket.clone(), e)),
        }
    }

    if clients.is_empty() && errors.is_empty() {
        errors.push((
            "(detección automática)".into(),
            anyhow!("no se encontró ningún socket de Docker ni de Podman"),
        ));
    }
    (clients, errors)
}
