use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use bollard::{
    container::{ListContainersOptions, StatsOptions},
    image::ListImagesOptions,
    network::ListNetworksOptions,
    Docker,
};
use futures_util::StreamExt;
use uuid::Uuid;

use crate::models::{
    container::DockerContainer,
    metrics::{ContainerMetrics, DockerInfo},
};

/// Build a Docker client from server configuration fields
pub fn make_docker_client(
    connection_type: &str,
    docker_host: Option<&str>,
    host: &str,
    port: i32,
) -> Result<Arc<Docker>> {
    let client = match connection_type {
        "local" => {
            let socket = docker_host.unwrap_or("unix:///var/run/docker.sock");
            let path = socket.strip_prefix("unix://").unwrap_or(socket);
            Docker::connect_with_unix(path, 120, bollard::API_DEFAULT_VERSION)
                .map_err(|e| anyhow!("Cannot connect to local Docker socket: {e}"))?
        }
        "tcp" => {
            let addr = if let Some(dh) = docker_host {
                dh.to_string()
            } else {
                format!("http://{host}:{port}")
            };
            Docker::connect_with_http(&addr, 120, bollard::API_DEFAULT_VERSION)
                .map_err(|e| anyhow!("Cannot connect to remote Docker via TCP: {e}"))?
        }
        other => return Err(anyhow!("Unsupported connection type: {other}")),
    };
    Ok(Arc::new(client))
}

/// Fetch Docker system info + version for a given client
pub async fn fetch_docker_info(
    docker: &Docker,
    server_id: Uuid,
) -> Result<DockerInfo> {
    let info = docker.info().await?;
    let ver = docker.version().await?;

    // bollard 0.16.x uses SCREAMING_SNAKE_CASE enum variants
    let swarm = info
        .swarm
        .as_ref()
        .and_then(|s| s.local_node_state.as_ref())
        .map(|s| matches!(s, bollard::models::LocalNodeState::ACTIVE))
        .unwrap_or(false);

    Ok(DockerInfo {
        server_id,
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

/// List all containers (running + stopped)
pub async fn list_containers(
    docker: &Docker,
    _server_id: Uuid,
) -> Result<Vec<DockerContainer>> {
    let raw = docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await?;

    let containers = raw
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
            let nets: Vec<String> = c
                .network_settings
                .as_ref()
                .and_then(|ns| ns.networks.as_ref())
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default();

            let ports_val = serde_json::to_value(&c.ports).unwrap_or(serde_json::json!([]));
            let labels_val = serde_json::to_value(&c.labels).unwrap_or(serde_json::json!({}));
            let mounts_val = serde_json::to_value(&c.mounts).unwrap_or(serde_json::json!([]));

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
                ports: ports_val,
                labels: labels_val,
                networks: nets,
                mounts: mounts_val,
            }
        })
        .collect();

    Ok(containers)
}

/// Collect resource stats for every running container
pub async fn collect_container_stats(
    docker: &Docker,
    server_id: Uuid,
) -> Result<Vec<ContainerMetrics>> {
    let running = docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: false,
            filters: {
                let mut f = HashMap::new();
                f.insert("status".to_string(), vec!["running".to_string()]);
                f
            },
            ..Default::default()
        }))
        .await?;

    let mut result = Vec::new();

    for c in running {
        let cid = match c.id.as_deref() {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => continue,
        };

        let mut stream = docker.stats(
            &cid,
            Some(StatsOptions {
                stream: false,
                one_shot: true,
            }),
        );

        if let Some(Ok(stats)) = stream.next().await {
            let cpu_delta = stats.cpu_stats.cpu_usage.total_usage
                .saturating_sub(stats.precpu_stats.cpu_usage.total_usage);
            let sys_delta = stats
                .cpu_stats
                .system_cpu_usage
                .unwrap_or(0)
                .saturating_sub(stats.precpu_stats.system_cpu_usage.unwrap_or(0));
            let num_cpus = stats
                .cpu_stats
                .online_cpus
                .unwrap_or(1) as f64;
            let cpu_pct = if sys_delta > 0 {
                (cpu_delta as f64 / sys_delta as f64) * num_cpus * 100.0
            } else {
                0.0
            };

            let mem_usage = stats.memory_stats.usage.unwrap_or(0) as i64;
            let mem_limit = stats.memory_stats.limit.unwrap_or(0) as i64;
            let mem_pct = if mem_limit > 0 {
                (mem_usage as f64 / mem_limit as f64) * 100.0
            } else {
                0.0
            };

            // Aggregate network I/O across all interfaces
            let (net_rx, net_tx) = stats
                .networks
                .as_ref()
                .map(|nets| {
                    nets.values().fold((0i64, 0i64), |acc, n| {
                        (acc.0 + n.rx_bytes as i64, acc.1 + n.tx_bytes as i64)
                    })
                })
                .unwrap_or((0, 0));

            // Aggregate block I/O
            let (blk_r, blk_w) = stats
                .blkio_stats
                .io_service_bytes_recursive
                .as_deref()
                .map(|entries| {
                    entries.iter().fold((0i64, 0i64), |acc, e| match e.op.as_str() {
                        "read" | "Read" => (acc.0 + e.value as i64, acc.1),
                        "write" | "Write" => (acc.0, acc.1 + e.value as i64),
                        _ => acc,
                    })
                })
                .unwrap_or((0, 0));

            let pids = stats.pids_stats.current.unwrap_or(0) as i64;

            result.push(ContainerMetrics {
                time: chrono::Utc::now(),
                container_id: cid,
                server_id,
                cpu_usage_percent: Some(cpu_pct),
                memory_used_bytes: Some(mem_usage),
                memory_limit_bytes: Some(mem_limit),
                memory_usage_percent: Some(mem_pct),
                net_rx_bytes: Some(net_rx),
                net_tx_bytes: Some(net_tx),
                block_read_bytes: Some(blk_r),
                block_write_bytes: Some(blk_w),
                pids: Some(pids),
            });
        }
    }

    Ok(result)
}

/// Fetch image list summary
pub async fn list_images(docker: &Docker) -> Result<serde_json::Value> {
    let images = docker
        .list_images(Some(ListImagesOptions::<String> {
            all: false,
            ..Default::default()
        }))
        .await?;
    Ok(serde_json::to_value(images)?)
}

/// Fetch network list summary
pub async fn list_networks(docker: &Docker) -> Result<serde_json::Value> {
    let networks = docker
        .list_networks(Some(ListNetworksOptions::<String> {
            filters: HashMap::new(),
        }))
        .await?;
    Ok(serde_json::to_value(networks)?)
}

/// Fetch volume list summary
pub async fn list_volumes(docker: &Docker) -> Result<serde_json::Value> {
    let volumes = docker
        .list_volumes(None::<bollard::volume::ListVolumesOptions<String>>)
        .await?;
    Ok(serde_json::to_value(volumes)?)
}
