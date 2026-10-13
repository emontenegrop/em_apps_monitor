use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use em_protocol::messages::{RuntimeDescriptor, RuntimeSnapshot, Snapshot};
use futures_util::future::join_all;

use crate::{
    collectors::{disks, host::HostCollector, runtime::RuntimeClient, stats::StatsCollector},
    config::AgentConfig,
};

struct RuntimeState {
    client: RuntimeClient,
    stats: StatsCollector,
    /// Hash de la última lista de contenedores enviada
    containers_hash: Option<u64>,
}

/// Arma los snapshots periódicos del host. Conserva el estado entre ciclos.
pub struct Collector {
    cfg: AgentConfig,
    host: HostCollector,
    runtimes: Vec<RuntimeState>,
    seq: u64,
}

impl Collector {
    pub fn new(cfg: AgentConfig, clients: Vec<RuntimeClient>) -> Self {
        Self {
            cfg,
            host: HostCollector::new(),
            runtimes: clients
                .into_iter()
                .map(|client| RuntimeState { client, stats: StatsCollector::default(), containers_hash: None })
                .collect(),
            seq: 0,
        }
    }

    pub fn runtime_count(&self) -> usize {
        self.runtimes.len()
    }

    /// Reemplaza los runtimes (tras una nueva detección); se pierde el historial de stats
    pub fn replace_runtimes(&mut self, clients: Vec<RuntimeClient>) {
        self.runtimes = clients
            .into_iter()
            .map(|client| RuntimeState { client, stats: StatsCollector::default(), containers_hash: None })
            .collect();
    }

    /// Clientes de los runtimes, para atender solicitudes del central
    pub fn clients(&self) -> crate::rpc::Clients {
        self.runtimes.iter().map(|r| (r.client.kind, r.client.docker.clone())).collect()
    }

    pub fn runtimes(&self) -> Vec<RuntimeDescriptor> {
        self.runtimes.iter().map(|r| r.client.descriptor()).collect()
    }

    /// Recolecta un snapshot. Con `force_containers = false`, la lista de contenedores
    /// solo se incluye si cambió desde el snapshot anterior.
    pub async fn snapshot(&mut self, force_containers: bool) -> Snapshot {
        let host_root = self.cfg.host_root.clone();
        let concurrency = self.cfg.stats_concurrency;

        // sysinfo lee /proc de forma bloqueante
        let host = tokio::task::block_in_place(|| self.host.collect());
        let disks = tokio::task::block_in_place(|| disks::collect(host_root.as_deref()));

        let runtimes = join_all(
            self.runtimes
                .iter_mut()
                .map(|rt| collect_runtime(rt, concurrency, force_containers)),
        )
        .await;

        self.seq += 1;
        Snapshot { seq: self.seq, ts: chrono::Utc::now(), host, disks, runtimes }
    }
}

async fn collect_runtime(rt: &mut RuntimeState, concurrency: usize, force_containers: bool) -> RuntimeSnapshot {
    let mut snap = RuntimeSnapshot {
        kind: rt.client.kind,
        socket: rt.client.socket.clone(),
        info: None,
        error: None,
        containers: None,
        stats: Vec::new(),
    };

    match rt.client.info().await {
        Ok(info) => snap.info = Some(info),
        Err(e) => {
            tracing::warn!("{:?} en {} no responde: {e}", rt.client.kind, rt.client.socket);
            snap.error = Some(e.to_string());
            // Al recuperarse se reenvía la lista completa
            rt.containers_hash = None;
            return snap;
        }
    }

    match rt.client.list_containers().await {
        Ok(containers) => {
            let hash = hash_json(&containers);
            if force_containers || rt.containers_hash != Some(hash) {
                rt.containers_hash = Some(hash);
                snap.containers = Some(containers);
            }
        }
        Err(e) => snap.error = Some(format!("listado de contenedores: {e}")),
    }

    match rt.stats.collect(&rt.client.docker, concurrency).await {
        Ok(stats) => snap.stats = stats,
        Err(e) => snap.error = Some(format!("stats: {e}")),
    }
    if let (None, Some(first)) = (&snap.error, rt.stats.failures.first()) {
        snap.error = Some(format!(
            "stats no disponibles en {} contenedor(es): {first}",
            rt.stats.failures.len()
        ));
    }
    snap
}

fn hash_json<T: serde::Serialize>(value: &T) -> u64 {
    let mut h = DefaultHasher::new();
    serde_json::to_string(value).unwrap_or_default().hash(&mut h);
    h.finish()
}
