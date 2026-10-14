//! Solicitudes bajo demanda del central. Todas son de solo lectura.

use std::{path::PathBuf, sync::Arc, time::Duration};

use bollard::{
    container::{InspectContainerOptions, LogsOptions},
    errors::Error as DockerError,
    image::ListImagesOptions,
    network::ListNetworksOptions,
    Docker,
};
use em_protocol::messages::{RequestMethod, RequestResult, RuntimeKind};
use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::collectors::disks;

/// Debe ser menor que la espera del central (15 s) para que el error llegue a tiempo
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_LOG_TAIL: u32 = 1000;
/// Tope de bytes de logs; se conservan las líneas más recientes
const MAX_LOG_BYTES: usize = 2 * 1024 * 1024;

/// Runtimes disponibles para atender solicitudes
pub type Clients = Vec<(RuntimeKind, Arc<Docker>)>;

pub async fn handle(method: RequestMethod, clients: Clients, host_root: Option<PathBuf>) -> RequestResult {
    match tokio::time::timeout(REQUEST_TIMEOUT, dispatch(method, &clients, host_root)).await {
        Ok(Ok(value)) => RequestResult::Ok(value),
        Ok(Err(msg)) => RequestResult::Error(msg),
        Err(_) => RequestResult::Error(format!("el runtime no respondió en {}s", REQUEST_TIMEOUT.as_secs())),
    }
}

async fn dispatch(method: RequestMethod, clients: &Clients, host_root: Option<PathBuf>) -> Result<Value, String> {
    match method {
        RequestMethod::Ping => Ok(json!({ "pong": true })),
        RequestMethod::Disks => {
            let list = tokio::task::spawn_blocking(move || disks::collect(host_root.as_deref()))
                .await
                .map_err(|e| e.to_string())?;
            to_json(&list)
        }
        RequestMethod::Inspect { runtime, container_id, reveal_env } => {
            let (_, mut value) = find_container(clients, runtime, &container_id).await?;
            if !reveal_env {
                em_protocol::redact::redact_inspect_env(&mut value);
            }
            Ok(value)
        }
        RequestMethod::Logs { runtime, container_id, tail } => {
            let (docker, _) = find_container(clients, runtime, &container_id).await?;
            logs(&docker, &container_id, tail.clamp(1, MAX_LOG_TAIL)).await
        }
        RequestMethod::Images { runtime } => {
            merge_lists(of_kind(clients, runtime)?, |d| async move {
                let images = d
                    .list_images(Some(ListImagesOptions::<String> { all: false, ..Default::default() }))
                    .await?;
                Ok(serde_json::to_value(images)?)
            })
            .await
        }
        RequestMethod::Networks { runtime } => {
            merge_lists(of_kind(clients, runtime)?, |d| async move {
                let nets = d.list_networks(None::<ListNetworksOptions<String>>).await?;
                Ok(serde_json::to_value(nets)?)
            })
            .await
        }
        RequestMethod::Volumes { runtime } => {
            merge_lists(of_kind(clients, runtime)?, |d| async move {
                let vols = d.list_volumes(None::<bollard::volume::ListVolumesOptions<String>>).await?;
                Ok(serde_json::to_value(vols)?)
            })
            .await
        }
    }
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

fn of_kind(clients: &Clients, kind: RuntimeKind) -> Result<Vec<Arc<Docker>>, String> {
    let list: Vec<_> = clients.iter().filter(|(k, _)| *k == kind).map(|(_, d)| d.clone()).collect();
    if list.is_empty() {
        Err(format!("este host no tiene {kind:?}"))
    } else {
        Ok(list)
    }
}

fn is_not_found(e: &DockerError) -> bool {
    matches!(e, DockerError::DockerResponseServerError { status_code: 404, .. })
}

/// Busca el contenedor en los runtimes del tipo pedido (puede haber Podman rootful y rootless)
async fn find_container(clients: &Clients, kind: RuntimeKind, id: &str) -> Result<(Arc<Docker>, Value), String> {
    for docker in of_kind(clients, kind)? {
        match docker.inspect_container(id, None::<InspectContainerOptions>).await {
            Ok(details) => return Ok((docker, to_json(&details)?)),
            Err(e) if is_not_found(&e) => continue,
            Err(e) => return Err(format!("inspect de {id}: {e}")),
        }
    }
    Err(format!("contenedor {id} no encontrado en {kind:?}"))
}

async fn logs(docker: &Docker, id: &str, tail: u32) -> Result<Value, String> {
    let mut stream = docker.logs(
        id,
        Some(LogsOptions::<String> {
            stdout: true,
            stderr: true,
            timestamps: true,
            tail: tail.to_string(),
            ..Default::default()
        }),
    );

    let mut lines = std::collections::VecDeque::new();
    let mut bytes = 0;
    while let Some(chunk) = stream.next().await {
        let line = chunk.map_err(|e| format!("logs de {id}: {e}"))?.to_string();
        bytes += line.len();
        lines.push_back(line);
        // Se descartan las más viejas para no pasar el tope del mensaje
        while bytes > MAX_LOG_BYTES {
            match lines.pop_front() {
                Some(old) => bytes -= old.len(),
                None => break,
            }
        }
    }
    to_json(&lines)
}

/// Ejecuta el listado en cada runtime y combina: arreglos concatenados y, para volúmenes,
/// el campo `Volumes` concatenado.
async fn merge_lists<F, Fut>(dockers: Vec<Arc<Docker>>, list: F) -> Result<Value, String>
where
    F: Fn(Arc<Docker>) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<Value>>,
{
    let mut results = Vec::with_capacity(dockers.len());
    for d in dockers {
        results.push(list(d).await.map_err(|e| format!("{e:#}"))?);
    }
    Ok(merge_values(results))
}

fn merge_values(mut results: Vec<Value>) -> Value {
    if results.len() == 1 {
        return results.pop().unwrap();
    }
    if results.iter().all(Value::is_array) {
        return Value::Array(results.into_iter().flat_map(|v| v.as_array().cloned().unwrap_or_default()).collect());
    }
    // ListVolumesResponse: { "Volumes": [...], "Warnings": [...] }
    let mut volumes = Vec::new();
    let mut warnings = Vec::new();
    for r in results {
        volumes.extend(r["Volumes"].as_array().cloned().unwrap_or_default());
        warnings.extend(r["Warnings"].as_array().cloned().unwrap_or_default());
    }
    json!({ "Volumes": volumes, "Warnings": warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_arrays_and_volume_responses() {
        assert_eq!(merge_values(vec![json!([1])]), json!([1]));
        assert_eq!(merge_values(vec![json!([1, 2]), json!([3])]), json!([1, 2, 3]));
        assert_eq!(
            merge_values(vec![
                json!({"Volumes": [{"Name": "a"}], "Warnings": null}),
                json!({"Volumes": [{"Name": "b"}], "Warnings": ["w"]}),
            ]),
            json!({"Volumes": [{"Name": "a"}, {"Name": "b"}], "Warnings": ["w"]})
        );
    }
}
