//! Persistencia de los snapshots que envían los agentes.

use std::{collections::BTreeMap, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use em_protocol::{
    container::DockerContainer,
    messages::{RuntimeDescriptor, RuntimeKind, Snapshot},
    metrics::{ContainerMetrics, DiskUsage},
};
use sqlx::{Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::{models::metrics::WsEvent, state::AppState};

/// Desfase máximo tolerado entre el reloj del agente y el del central
const MAX_CLOCK_SKEW_SECS: i64 = 120;

pub fn runtime_name(kind: RuntimeKind) -> &'static str {
    match kind {
        RuntimeKind::Docker => "docker",
        RuntimeKind::Podman => "podman",
    }
}

/// Estado de ingesta de una conexión de agente
pub struct SessionIngest {
    server_id: Uuid,
    runtimes: Vec<RuntimeDescriptor>,
    /// Última lista de contenedores por runtime, para emitir la lista completa del servidor
    containers: BTreeMap<&'static str, Vec<DockerContainer>>,
    skew_warned: bool,
}

impl SessionIngest {
    pub fn new(server_id: Uuid, runtimes: Vec<RuntimeDescriptor>) -> Self {
        Self { server_id, runtimes, containers: BTreeMap::new(), skew_warned: false }
    }

    /// Hora con la que se guarda el snapshot. Los snapshots en vivo con un reloj muy
    /// desfasado usan la hora del central; los de backfill solo se corrigen si vienen del futuro.
    fn effective_time(&mut self, ts: DateTime<Utc>, live: bool) -> DateTime<Utc> {
        let now = Utc::now();
        let skew = ts - now;
        let max = Duration::seconds(MAX_CLOCK_SKEW_SECS);
        if skew > max || (live && skew < -max) {
            if !self.skew_warned {
                tracing::warn!(
                    "reloj del agente {} desfasado {}s: se usa la hora del central",
                    self.server_id,
                    skew.num_seconds()
                );
                self.skew_warned = true;
            }
            now
        } else {
            ts
        }
    }

    /// Guarda un snapshot. Los de backfill solo aportan series de tiempo; el estado
    /// actual (contenedores, info, discos) lo actualizan los snapshots en vivo.
    pub async fn ingest(&mut self, state: &Arc<AppState>, mut snap: Snapshot, live: bool) -> anyhow::Result<()> {
        let sid = self.server_id;
        let time = self.effective_time(snap.ts, live);

        snap.host.server_id = sid;
        snap.host.time = time;
        let mut stats: Vec<ContainerMetrics> = Vec::new();
        for rt in &mut snap.runtimes {
            for s in rt.stats.drain(..) {
                stats.push(ContainerMetrics { server_id: sid, time, ..s });
            }
        }

        let mut tx = state.db.begin().await?;
        insert_host_metrics(&mut tx, &snap.host).await?;
        insert_container_metrics(&mut tx, &stats).await?;

        let mut containers_changed = false;
        if live {
            for rt in &mut snap.runtimes {
                let name = runtime_name(rt.kind);
                if let Some(info) = rt.info.as_mut() {
                    info.server_id = sid;
                    upsert_runtime_info(&mut tx, name, info).await?;
                }
                if let Some(list) = rt.containers.take() {
                    replace_containers(&mut tx, sid, name, &list).await?;
                    self.containers.insert(name, list);
                    containers_changed = true;
                }
            }
            replace_disks(&mut tx, sid, &snap.disks).await?;

            sqlx::query(
                "UPDATE servers SET status = 'online', last_seen_at = NOW(), updated_at = NOW(), runtimes = $2 WHERE id = $1",
            )
            .bind(sid)
            .bind(self.runtime_status(&snap))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        if live {
            broadcast(state, &WsEvent::HostMetrics { server_id: sid, data: snap.host });
            for rt in snap.runtimes {
                if let Some(info) = rt.info {
                    let runtime = Some(runtime_name(rt.kind).to_string());
                    broadcast(state, &WsEvent::DockerInfo { server_id: sid, runtime, data: info });
                }
            }
            if containers_changed {
                let all = self.containers.values().flatten().cloned().collect();
                broadcast(state, &WsEvent::Containers { server_id: sid, data: all });
            }
            if !stats.is_empty() {
                broadcast(state, &WsEvent::ContainerMetrics { server_id: sid, data: stats });
            }
        }
        Ok(())
    }

    /// Runtimes del snapshot en vivo, con su error de este ciclo. La versión sale de la
    /// info del ciclo o, si el runtime no respondió, de lo que informó el Hello.
    fn runtime_status(&self, snap: &Snapshot) -> serde_json::Value {
        let list: Vec<serde_json::Value> = snap
            .runtimes
            .iter()
            .map(|r| {
                let hello = self.runtimes.iter().find(|d| d.kind == r.kind && d.socket == r.socket);
                let version = r
                    .info
                    .as_ref()
                    .and_then(|i| i.docker_version.clone())
                    .or_else(|| hello.and_then(|d| d.version.clone()));
                serde_json::json!({
                    "kind": r.kind,
                    "socket": r.socket,
                    "version": version,
                    "rootless": r.rootless || hello.is_some_and(|d| d.rootless),
                    "error": r.error,
                })
            })
            .collect();
        serde_json::Value::Array(list)
    }
}

pub fn broadcast(state: &Arc<AppState>, event: &WsEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        let _ = state.broadcast_tx.send(json);
    }
}

type Tx<'a> = Transaction<'a, Postgres>;

async fn insert_host_metrics(tx: &mut Tx<'_>, m: &crate::models::metrics::HostMetrics) -> sqlx::Result<()> {
    sqlx::query(
        r#"INSERT INTO host_metrics
           (time, server_id, cpu_usage_percent, memory_used_bytes, memory_total_bytes,
            memory_usage_percent, swap_used_bytes, swap_total_bytes, net_rx_bytes, net_tx_bytes,
            load_avg_1, load_avg_5, load_avg_15, process_count, uptime_seconds)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)"#,
    )
    .bind(m.time)
    .bind(m.server_id)
    .bind(m.cpu_usage_percent)
    .bind(m.memory_used_bytes)
    .bind(m.memory_total_bytes)
    .bind(m.memory_usage_percent)
    .bind(m.swap_used_bytes)
    .bind(m.swap_total_bytes)
    .bind(m.net_rx_bytes)
    .bind(m.net_tx_bytes)
    .bind(m.load_avg_1)
    .bind(m.load_avg_5)
    .bind(m.load_avg_15)
    .bind(m.process_count)
    .bind(m.uptime_seconds)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_container_metrics(tx: &mut Tx<'_>, stats: &[ContainerMetrics]) -> sqlx::Result<()> {
    // 12 parámetros por fila: lotes de 1000 quedan lejos del límite de 65535 de Postgres
    for chunk in stats.chunks(1000) {
        let mut qb = QueryBuilder::<Postgres>::new(
            "INSERT INTO container_metrics (time, container_id, server_id, cpu_usage_percent, \
             memory_used_bytes, memory_limit_bytes, memory_usage_percent, net_rx_bytes, net_tx_bytes, \
             block_read_bytes, block_write_bytes, pids) ",
        );
        qb.push_values(chunk, |mut b, m| {
            b.push_bind(m.time)
                .push_bind(&m.container_id)
                .push_bind(m.server_id)
                .push_bind(m.cpu_usage_percent)
                .push_bind(m.memory_used_bytes)
                .push_bind(m.memory_limit_bytes)
                .push_bind(m.memory_usage_percent)
                .push_bind(m.net_rx_bytes)
                .push_bind(m.net_tx_bytes)
                .push_bind(m.block_read_bytes)
                .push_bind(m.block_write_bytes)
                .push_bind(m.pids);
        });
        qb.build().execute(&mut **tx).await?;
    }
    Ok(())
}

async fn upsert_runtime_info(tx: &mut Tx<'_>, runtime: &str, info: &crate::models::metrics::DockerInfo) -> sqlx::Result<()> {
    sqlx::query(
        r#"INSERT INTO docker_info
            (server_id, runtime_kind, docker_version, api_version, os_type, arch, kernel_version,
             containers_total, containers_running, containers_paused, containers_stopped,
             images_total, storage_driver, runtime, docker_root_dir, swarm_active, raw_info, updated_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,NOW())
           ON CONFLICT (server_id, runtime_kind) DO UPDATE SET
             docker_version=$3, api_version=$4, os_type=$5, arch=$6, kernel_version=$7,
             containers_total=$8, containers_running=$9, containers_paused=$10, containers_stopped=$11,
             images_total=$12, storage_driver=$13, runtime=$14, docker_root_dir=$15,
             swarm_active=$16, raw_info=$17, updated_at=NOW()"#,
    )
    .bind(info.server_id)
    .bind(runtime)
    .bind(&info.docker_version)
    .bind(&info.api_version)
    .bind(&info.os_type)
    .bind(&info.arch)
    .bind(&info.kernel_version)
    .bind(info.containers_total)
    .bind(info.containers_running)
    .bind(info.containers_paused)
    .bind(info.containers_stopped)
    .bind(info.images_total)
    .bind(&info.storage_driver)
    .bind(&info.runtime)
    .bind(&info.docker_root_dir)
    .bind(info.swarm_active)
    .bind(&info.raw_info)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn replace_containers(tx: &mut Tx<'_>, server_id: Uuid, runtime: &str, list: &[DockerContainer]) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM containers WHERE server_id = $1 AND runtime_kind = $2")
        .bind(server_id)
        .bind(runtime)
        .execute(&mut **tx)
        .await?;

    for chunk in list.chunks(1000) {
        let mut qb = QueryBuilder::<Postgres>::new(
            "INSERT INTO containers (id, server_id, runtime_kind, short_id, name, image, image_id, status, state, \
             container_created_at, ports, labels, networks, mounts, command, updated_at) ",
        );
        qb.push_values(chunk, |mut b, c| {
            b.push_bind(&c.id)
                .push_bind(server_id)
                .push_bind(runtime)
                .push_bind(&c.short_id)
                .push_bind(&c.name)
                .push_bind(&c.image)
                .push_bind(&c.image_id)
                .push_bind(&c.status)
                .push_bind(&c.state)
                .push_bind(DateTime::from_timestamp(c.created, 0))
                .push_bind(&c.ports)
                .push_bind(&c.labels)
                .push_bind(serde_json::to_value(&c.networks).unwrap_or_default())
                .push_bind(&c.mounts)
                .push_bind(&c.command)
                .push("NOW()");
        });
        qb.build().execute(&mut **tx).await?;
    }
    Ok(())
}

async fn replace_disks(tx: &mut Tx<'_>, server_id: Uuid, disks: &[DiskUsage]) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM disk_usage_latest WHERE server_id = $1")
        .bind(server_id)
        .execute(&mut **tx)
        .await?;
    if disks.is_empty() {
        return Ok(());
    }
    let mut qb = QueryBuilder::<Postgres>::new(
        "INSERT INTO disk_usage_latest (server_id, mount_point, name, fs_type, total_bytes, used_bytes, \
         available_bytes, usage_percent, updated_at) ",
    );
    qb.push_values(disks, |mut b, d| {
        b.push_bind(server_id)
            .push_bind(&d.mount_point)
            .push_bind(&d.name)
            .push_bind(&d.fs_type)
            .push_bind(d.total_bytes as i64)
            .push_bind(d.used_bytes as i64)
            .push_bind(d.available_bytes as i64)
            .push_bind(d.usage_percent)
            .push("NOW()");
    });
    // El mismo punto de montaje no debería repetirse, pero no se confía en el agente
    qb.push(" ON CONFLICT (server_id, mount_point) DO NOTHING");
    qb.build().execute(&mut **tx).await?;
    Ok(())
}
