use std::{sync::Arc, time::Duration};

use tokio::time::interval;
use uuid::Uuid;

use crate::{models::metrics::WsEvent, state::AppState};

/// Tareas periódicas del central. Los datos los envían los agentes; aquí solo se
/// detectan agentes inactivos y se poda el historial. Nunca hace panic: los errores se registran.
pub async fn start_background_tasks(state: Arc<AppState>, interval_secs: u64, retention_days: i32) {
    // Agentes sin snapshots recientes (conexiones medio abiertas)
    let state_clone = state.clone();
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(interval_secs));
        loop {
            tick.tick().await;
            mark_stale_agents_offline(&state_clone, interval_secs * 3).await;
        }
    });

    // Prune old metrics once per hour
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            if let Err(e) = prune_old_metrics(&state, retention_days).await {
                tracing::warn!("Metrics pruning failed: {e}");
            }
        }
    });
}

async fn mark_stale_agents_offline(state: &Arc<AppState>, max_age_secs: u64) {
    let stale: Vec<Uuid> = match sqlx::query_scalar(
        r#"UPDATE servers SET status = 'offline', updated_at = NOW()
           WHERE agent_secret_hash IS NOT NULL AND status = 'online'
             AND (last_seen_at IS NULL OR last_seen_at < NOW() - make_interval(secs => $1))
           RETURNING id"#,
    )
    .bind(max_age_secs as f64)
    .fetch_all(&state.db)
    .await
    {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!("marcando agentes inactivos: {e}");
            return;
        }
    };
    for server_id in stale {
        tracing::warn!("agente {server_id} sin snapshots en {max_age_secs}s: offline");
        crate::agents::ingest::broadcast(
            state,
            &WsEvent::ServerStatus { server_id, status: "offline".into() },
        );
    }
}

async fn prune_old_metrics(state: &Arc<AppState>, retention_days: i32) -> anyhow::Result<()> {
    for table in ["host_metrics", "container_metrics", "disk_metrics"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE time < NOW() - make_interval(days => $1)"))
            .bind(retention_days)
            .execute(&state.db)
            .await?;
    }
    let sessions = crate::auth::prune_sessions(&state.db).await?;
    tracing::debug!("Old metrics pruned ({retention_days} days kept, {sessions} expired sessions)");
    Ok(())
}
