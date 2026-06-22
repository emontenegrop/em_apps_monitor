use std::{sync::Arc, time::Duration};

use tokio::time::interval;
use uuid::Uuid;

use crate::{
    collectors::{
        docker::{
            collect_container_stats, fetch_docker_info, list_containers, make_docker_client,
        },
        host::collect_local_host_metrics,
    },
    models::metrics::WsEvent,
    state::AppState,
};

/// Spawn the background collection loop. Never panics — errors are logged.
pub async fn start_collectors(state: Arc<AppState>, interval_secs: u64) {
    let state_clone = state.clone();
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(interval_secs));
        loop {
            tick.tick().await;
            if let Err(e) = run_collection_cycle(&state_clone).await {
                tracing::error!("Collection cycle failed: {e}");
            }
        }
    });

    // Prune old metrics once per hour
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            if let Err(e) = prune_old_metrics(&state).await {
                tracing::warn!("Metrics pruning failed: {e}");
            }
        }
    });
}

async fn run_collection_cycle(state: &Arc<AppState>) -> anyhow::Result<()> {
    let servers = sqlx::query_as::<_, crate::models::server::Server>(
        "SELECT * FROM servers WHERE is_active = true",
    )
    .fetch_all(&state.db)
    .await?;

    for server in servers {
        let sid: Uuid = server.id;

        // Ensure a Docker client is cached for this server
        {
            let has = state.docker_clients.read().await.contains_key(&sid);
            if !has {
                match make_docker_client(
                    &server.connection_type,
                    server.docker_host.as_deref(),
                    &server.host,
                    server.port,
                ) {
                    Ok(client) => {
                        state.docker_clients.write().await.insert(sid, client);
                    }
                    Err(e) => {
                        tracing::warn!("Cannot build Docker client for {}: {e}", server.name);
                        set_server_status(state, sid, "offline").await;
                        continue;
                    }
                }
            }
        }

        let docker = {
            let guard = state.docker_clients.read().await;
            guard.get(&sid).cloned()
        };

        let docker = match docker {
            Some(d) => d,
            None => continue,
        };

        // ── Docker info ──────────────────────────────────────────────
        match fetch_docker_info(&docker, sid).await {
            Ok(info) => {
                persist_docker_info(state, &info).await;
                let _ = state.broadcast_tx.send(
                    serde_json::to_string(&WsEvent::DockerInfo {
                        server_id: sid,
                        data: info,
                    })
                    .unwrap_or_default(),
                );
                set_server_status(state, sid, "online").await;
            }
            Err(e) => {
                tracing::warn!("Docker info failed for {}: {e}", server.name);
                set_server_status(state, sid, "offline").await;
                // Remove stale client so it is recreated next cycle
                state.docker_clients.write().await.remove(&sid);
                continue;
            }
        }

        // ── Container list ───────────────────────────────────────────
        if let Ok(containers) = list_containers(&docker, sid).await {
            persist_containers(state, sid, &containers).await;
            let _ = state.broadcast_tx.send(
                serde_json::to_string(&WsEvent::Containers {
                    server_id: sid,
                    data: containers,
                })
                .unwrap_or_default(),
            );
        }

        // ── Container stats ──────────────────────────────────────────
        if let Ok(stats) = collect_container_stats(&docker, sid).await {
            for s in &stats {
                persist_container_metrics(state, s).await;
            }
            let _ = state.broadcast_tx.send(
                serde_json::to_string(&WsEvent::ContainerMetrics {
                    server_id: sid,
                    data: stats,
                })
                .unwrap_or_default(),
            );
        }

        // ── Host metrics (local only) ────────────────────────────────
        if server.connection_type == "local" {
            match collect_local_host_metrics(sid) {
                Ok(metrics) => {
                    persist_host_metrics(state, &metrics).await;
                    let _ = state.broadcast_tx.send(
                        serde_json::to_string(&WsEvent::HostMetrics {
                            server_id: sid,
                            data: metrics,
                        })
                        .unwrap_or_default(),
                    );
                }
                Err(e) => tracing::warn!("Host metrics failed: {e}"),
            }
        }
    }

    Ok(())
}

// ── Persistence helpers ────────────────────────────────────────────────────

async fn set_server_status(state: &Arc<AppState>, server_id: Uuid, status: &str) {
    let _ = sqlx::query(
        "UPDATE servers SET status = $1, last_seen_at = CASE WHEN $1 = 'online' THEN NOW() ELSE last_seen_at END, updated_at = NOW() WHERE id = $2",
    )
    .bind(status)
    .bind(server_id)
    .execute(&state.db)
    .await;
}

async fn persist_docker_info(
    state: &Arc<AppState>,
    info: &crate::models::metrics::DockerInfo,
) {
    let _ = sqlx::query(
        r#"INSERT INTO docker_info
            (server_id, docker_version, api_version, os_type, arch, kernel_version,
             containers_total, containers_running, containers_paused, containers_stopped,
             images_total, storage_driver, runtime, docker_root_dir, swarm_active, raw_info, updated_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,NOW())
           ON CONFLICT (server_id) DO UPDATE SET
             docker_version=$2, api_version=$3, os_type=$4, arch=$5, kernel_version=$6,
             containers_total=$7, containers_running=$8, containers_paused=$9, containers_stopped=$10,
             images_total=$11, storage_driver=$12, runtime=$13, docker_root_dir=$14,
             swarm_active=$15, raw_info=$16, updated_at=NOW()"#,
    )
    .bind(info.server_id)
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
    .execute(&state.db)
    .await;
}

async fn persist_containers(
    state: &Arc<AppState>,
    server_id: Uuid,
    containers: &[crate::models::container::DockerContainer],
) {
    // Replace all containers for this server
    let _ = sqlx::query("DELETE FROM containers WHERE server_id = $1")
        .bind(server_id)
        .execute(&state.db)
        .await;

    for c in containers {
        let created_at = chrono::DateTime::from_timestamp(c.created, 0);
        let nets_json = serde_json::to_value(&c.networks).unwrap_or(serde_json::json!([]));
        let _ = sqlx::query(
            r#"INSERT INTO containers
               (id, server_id, short_id, name, image, image_id, status, state,
                container_created_at, ports, labels, networks, mounts, command, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,NOW())
               ON CONFLICT (id, server_id) DO UPDATE SET
                 name=$4, image=$5, image_id=$6, status=$7, state=$8,
                 container_created_at=$9, ports=$10, labels=$11, networks=$12,
                 mounts=$13, command=$14, updated_at=NOW()"#,
        )
        .bind(&c.id)
        .bind(server_id)
        .bind(&c.short_id)
        .bind(&c.name)
        .bind(&c.image)
        .bind(&c.image_id)
        .bind(&c.status)
        .bind(&c.state)
        .bind(created_at)
        .bind(&c.ports)
        .bind(&c.labels)
        .bind(nets_json)
        .bind(&c.mounts)
        .bind(&c.command)
        .execute(&state.db)
        .await;
    }
}

async fn persist_host_metrics(
    state: &Arc<AppState>,
    m: &crate::models::metrics::HostMetrics,
) {
    let _ = sqlx::query(
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
    .execute(&state.db)
    .await;
}

async fn persist_container_metrics(
    state: &Arc<AppState>,
    m: &crate::models::metrics::ContainerMetrics,
) {
    let _ = sqlx::query(
        r#"INSERT INTO container_metrics
           (time, container_id, server_id, cpu_usage_percent, memory_used_bytes, memory_limit_bytes,
            memory_usage_percent, net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
    )
    .bind(m.time)
    .bind(&m.container_id)
    .bind(m.server_id)
    .bind(m.cpu_usage_percent)
    .bind(m.memory_used_bytes)
    .bind(m.memory_limit_bytes)
    .bind(m.memory_usage_percent)
    .bind(m.net_rx_bytes)
    .bind(m.net_tx_bytes)
    .bind(m.block_read_bytes)
    .bind(m.block_write_bytes)
    .bind(m.pids)
    .execute(&state.db)
    .await;
}

async fn prune_old_metrics(state: &Arc<AppState>) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM host_metrics WHERE time < NOW() - INTERVAL '7 days'")
        .execute(&state.db)
        .await?;
    sqlx::query("DELETE FROM container_metrics WHERE time < NOW() - INTERVAL '7 days'")
        .execute(&state.db)
        .await?;
    tracing::debug!("Old metrics pruned");
    Ok(())
}
