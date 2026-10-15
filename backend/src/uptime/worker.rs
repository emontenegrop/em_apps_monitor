//! Tarea de fondo: verifica cada objetivo activo según su intervalo, guarda el resultado,
//! actualiza su estado y notifica los cambios (caída / recuperación).

use std::{sync::Arc, time::Duration};

use tokio::sync::Semaphore;
use uuid::Uuid;

use super::{
    checker::{self, CheckOutcome},
    model::{pending_alert, transition, CheckRecord, StateChange, Target},
    notify,
};
use crate::{agents::ingest::broadcast, models::metrics::WsEvent, state::AppState};

/// Cada cuánto se buscan objetivos con la verificación vencida
const TICK: Duration = Duration::from_secs(5);
/// Verificaciones simultáneas como máximo
const MAX_CONCURRENT: usize = 16;
/// Días de historial de verificaciones que se conservan
pub const RETENTION_DAYS: i32 = 7;

pub fn start(state: Arc<AppState>) {
    let limit = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let st = state.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            if let Err(e) = dispatch_due(&st, &limit).await {
                tracing::warn!("disponibilidad: no se pudieron buscar objetivos pendientes: {e}");
            }
        }
    });

    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            let res = sqlx::query("DELETE FROM target_checks WHERE time < NOW() - make_interval(days => $1)")
                .bind(RETENTION_DAYS)
                .execute(&state.db)
                .await;
            if let Err(e) = res {
                tracing::warn!("disponibilidad: no se pudo podar el historial: {e}");
            }
        }
    });
}

async fn dispatch_due(state: &Arc<AppState>, limit: &Arc<Semaphore>) -> sqlx::Result<()> {
    let due = sqlx::query_as::<_, Target>(
        r#"SELECT * FROM monitored_targets
           WHERE is_active
             AND (last_check_at IS NULL
                  OR last_check_at + make_interval(secs => check_interval_secs) <= NOW())"#,
    )
    .fetch_all(&state.db)
    .await?;

    for target in due {
        let Some(guard) = InflightGuard::acquire(state, target.id) else { continue };
        let (state, limit) = (state.clone(), limit.clone());
        tokio::spawn(async move {
            let _guard = guard;
            let Ok(_permit) = limit.acquire_owned().await else { return };
            if let Err(e) = run_check(&state, &target).await {
                tracing::warn!("disponibilidad: verificación de {} falló al guardarse: {e}", target.name);
            }
        });
    }
    Ok(())
}

/// Marca un objetivo como "en verificación" mientras exista; evita verificarlo dos veces a la vez
pub struct InflightGuard {
    state: Arc<AppState>,
    id: Uuid,
}

impl InflightGuard {
    pub fn acquire(state: &Arc<AppState>, id: Uuid) -> Option<Self> {
        let mut set = state.uptime_inflight.lock().unwrap_or_else(|e| e.into_inner());
        set.insert(id).then(|| Self { state: state.clone(), id })
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        let mut set = self.state.uptime_inflight.lock().unwrap_or_else(|e| e.into_inner());
        set.remove(&self.id);
    }
}

/// Verifica un objetivo y guarda el resultado. `None` si el objetivo se eliminó mientras tanto.
pub async fn run_check(state: &Arc<AppState>, target: &Target) -> anyhow::Result<Option<CheckRecord>> {
    let time = chrono::Utc::now();
    let timeout = Duration::from_secs(target.timeout_secs.clamp(1, 60) as u64);
    let outcome: CheckOutcome = match target.monitor_type.as_str() {
        "ping" => checker::check_ping(&target.url, timeout).await,
        _ => checker::check_http(&target.url, timeout, &target.expected_status_codes).await,
    };

    let mut tx = state.db.begin().await?;
    // Estado actual con bloqueo: una verificación manual y la periódica no se pisan
    let Some(current) = sqlx::query_as::<_, Target>("SELECT * FROM monitored_targets WHERE id = $1 FOR UPDATE")
        .bind(target.id)
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(None);
    };

    let (status, failures) = transition(current.consecutive_failures, current.alert_threshold, outcome.is_success);

    sqlx::query(
        r#"INSERT INTO target_checks
           (time, target_id, is_success, response_ms, error_message, status_code, ssl_valid,
            ssl_expiry_days, redirect_url, packet_loss, ttl)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
    )
    .bind(time)
    .bind(target.id)
    .bind(outcome.is_success)
    .bind(outcome.response_ms)
    .bind(&outcome.error_message)
    .bind(outcome.status_code)
    .bind(outcome.ssl_valid)
    .bind(outcome.ssl_expiry_days)
    .bind(&outcome.redirect_url)
    .bind(outcome.packet_loss)
    .bind(outcome.ttl)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"UPDATE monitored_targets
           SET last_status = $2, consecutive_failures = $3, last_check_at = $4,
               last_response_ms = $5, updated_at = NOW()
           WHERE id = $1"#,
    )
    .bind(target.id)
    .bind(status)
    .bind(failures)
    .bind(time)
    .bind(outcome.response_ms)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let record = CheckRecord {
        target_id: target.id,
        time,
        is_success: outcome.is_success,
        response_ms: outcome.response_ms,
        error_message: outcome.error_message,
        status_code: outcome.status_code,
        ssl_valid: outcome.ssl_valid,
        ssl_expiry_days: outcome.ssl_expiry_days,
        redirect_url: outcome.redirect_url,
        packet_loss: outcome.packet_loss,
        ttl: outcome.ttl,
        status: status.to_string(),
        consecutive_failures: failures,
    };

    broadcast(state, &WsEvent::TargetCheck { target_id: target.id, status: status.to_string() });

    if current.last_status.as_deref() != Some(status) {
        tracing::info!("disponibilidad: {} → {status}", current.name);
    }

    // Se espera el envío (no se lanza aparte): mientras dura, este objetivo sigue marcado
    // "en verificación" y la siguiente verificación no puede enviar la misma alerta.
    if let Some(change) = pending_alert(status, current.notified_status.as_deref()) {
        if notify::send_state_alert(state, &current, &record, change).await {
            let notified = match change {
                StateChange::ToOffline => "offline",
                StateChange::ToOnline => "online",
            };
            sqlx::query("UPDATE monitored_targets SET notified_status = $2 WHERE id = $1")
                .bind(target.id)
                .bind(notified)
                .execute(&state.db)
                .await?;
        }
    }
    Ok(Some(record))
}
