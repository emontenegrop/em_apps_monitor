//! `GET /api/agent/ws`: sesión WebSocket de un agente.

use std::{borrow::Cow, sync::Arc, time::Duration};

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use em_protocol::messages::{
    AgentMessage, CentralMessage, RuntimeDescriptor, CLOSE_DUPLICATE, CLOSE_INCOMPATIBLE_PROTOCOL,
};
use futures_util::{SinkExt, StreamExt};
use tokio::{sync::mpsc, time::Instant};
use uuid::Uuid;

use super::{
    auth::{self, AuthError},
    ingest::{broadcast, SessionIngest},
    registry::Outbound,
};
use crate::{models::metrics::WsEvent, state::AppState};

/// Versión mínima del protocolo aceptada
pub const MIN_PROTOCOL_VERSION: u32 = 1;
/// Tamaño máximo de un mensaje del agente
const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const PING_EVERY: Duration = Duration::from_secs(30);
/// Sin tráfico del agente durante este tiempo, la conexión se da por muerta
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);

pub async fn agent_ws(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let server_id = match auth::authenticate(&state.db, &headers).await {
        Ok(id) => id,
        Err(AuthError::Database(e)) => {
            tracing::error!("autenticación de agente: {e}");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        Err(e) => {
            tracing::warn!("conexión de agente rechazada: {e}");
            return (StatusCode::UNAUTHORIZED, e.to_string()).into_response();
        }
    };

    ws.max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move {
            if let Err(e) = run_session(state, server_id, socket).await {
                tracing::warn!("sesión del agente {server_id} terminó con error: {e:#}");
            }
        })
}

fn close_msg(code: u16, reason: &'static str) -> Message {
    Message::Close(Some(CloseFrame { code, reason: Cow::Borrowed(reason) }))
}

async fn run_session(state: Arc<AppState>, server_id: Uuid, socket: WebSocket) -> anyhow::Result<()> {
    let (mut sink, mut stream) = socket.split();

    // ── Hello ────────────────────────────────────────────────────────────
    let hello = match tokio::time::timeout(HELLO_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => serde_json::from_str::<AgentMessage>(&text)?,
        _ => anyhow::bail!("el agente no envió Hello a tiempo"),
    };
    let AgentMessage::Hello { protocol_version, agent_version, hostname, os, arch, kernel: _, runtimes } = hello else {
        anyhow::bail!("el primer mensaje no fue Hello");
    };
    if protocol_version < MIN_PROTOCOL_VERSION {
        let _ = sink
            .send(close_msg(CLOSE_INCOMPATIBLE_PROTOCOL, "versión de protocolo no soportada"))
            .await;
        anyhow::bail!("protocolo {protocol_version} < mínimo {MIN_PROTOCOL_VERSION}");
    }

    // ── Registro ─────────────────────────────────────────────────────────
    let conn_id = Uuid::new_v4();
    let (tx, mut rx) = mpsc::channel::<Outbound>(64);
    if let Err(owner) = state.agents.register(server_id, conn_id, &hostname, tx.clone()).await {
        let _ = sink
            .send(close_msg(CLOSE_DUPLICATE, "credencial en uso por otro host"))
            .await;
        anyhow::bail!("credencial duplicada: {hostname} intentó conectar mientras {owner} está conectado");
    }

    sqlx::query(
        r#"UPDATE servers SET agent_version = $2, agent_protocol_version = $3, hostname = $4, os = $5,
             arch = $6, runtimes = $7, last_connected_at = NOW(), last_seen_at = NOW(),
             status = 'online', updated_at = NOW()
           WHERE id = $1"#,
    )
    .bind(server_id)
    .bind(&agent_version)
    .bind(protocol_version as i32)
    .bind(&hostname)
    .bind(&os)
    .bind(&arch)
    .bind(serde_json::to_value(&runtimes)?)
    .execute(&state.db)
    .await?;

    tracing::info!(
        "agente conectado: {server_id} ({hostname}, v{agent_version}, runtimes: {})",
        describe(&runtimes)
    );

    // ── Tarea de escritura ───────────────────────────────────────────────
    let mut writer = tokio::spawn(async move {
        let mut ping = tokio::time::interval(PING_EVERY);
        loop {
            tokio::select! {
                out = rx.recv() => match out {
                    Some(Outbound::Message(msg)) => {
                        let text = match serde_json::to_string(&msg) {
                            Ok(t) => t,
                            Err(e) => { tracing::error!("serializando mensaje al agente: {e}"); continue; }
                        };
                        if sink.send(Message::Text(text)).await.is_err() { break; }
                    }
                    Some(Outbound::Close { code, reason }) => {
                        let _ = sink.send(close_msg(code, reason)).await;
                        break;
                    }
                    None => break,
                },
                _ = ping.tick() => {
                    if sink.send(Message::Ping(Vec::new())).await.is_err() { break; }
                }
            }
        }
    });

    let _ = tx
        .send(Outbound::Message(CentralMessage::Welcome {
            server_id,
            collection_interval_secs: state.collection_interval_secs,
            min_protocol_version: MIN_PROTOCOL_VERSION,
        }))
        .await;
    broadcast(&state, &WsEvent::ServerStatus { server_id, status: "online".into() });

    // ── Lectura ──────────────────────────────────────────────────────────
    let mut ingest = SessionIngest::new(server_id, runtimes);
    let result = read_loop(&state, &mut stream, &mut ingest, &mut writer).await;

    writer.abort();
    if state.agents.unregister(server_id, conn_id).await {
        sqlx::query("UPDATE servers SET status = 'offline', updated_at = NOW() WHERE id = $1")
            .bind(server_id)
            .execute(&state.db)
            .await?;
        broadcast(&state, &WsEvent::ServerStatus { server_id, status: "offline".into() });
        tracing::info!("agente desconectado: {server_id}");
    }
    result
}

async fn read_loop(
    state: &Arc<AppState>,
    stream: &mut futures_util::stream::SplitStream<WebSocket>,
    ingest: &mut SessionIngest,
    writer: &mut tokio::task::JoinHandle<()>,
) -> anyhow::Result<()> {
    let mut deadline = Instant::now() + IDLE_TIMEOUT;
    loop {
        let next = tokio::select! {
            // La tarea de escritura terminó: cierre pedido por el central o socket roto
            _ = &mut *writer => return Ok(()),
            next = tokio::time::timeout_at(deadline, stream.next()) => next,
        };
        let msg = match next {
            Err(_) => anyhow::bail!("sin tráfico del agente en {}s", IDLE_TIMEOUT.as_secs()),
            Ok(None) => return Ok(()),
            Ok(Some(Err(e))) => return Err(e.into()),
            Ok(Some(Ok(m))) => m,
        };
        deadline = Instant::now() + IDLE_TIMEOUT;

        let text = match msg {
            Message::Text(t) => t,
            Message::Close(frame) => {
                tracing::debug!("el agente cerró la conexión: {frame:?}");
                return Ok(());
            }
            // Ping y Pong solo renuevan el plazo de inactividad
            _ => continue,
        };

        match serde_json::from_str::<AgentMessage>(&text) {
            Ok(AgentMessage::Snapshot(snap)) => {
                if let Err(e) = ingest.ingest(state, snap, true).await {
                    tracing::error!("guardando snapshot: {e:#}");
                }
            }
            Ok(AgentMessage::Backfill { snapshots }) => {
                let n = snapshots.len();
                for snap in snapshots {
                    if let Err(e) = ingest.ingest(state, snap, false).await {
                        tracing::error!("guardando backfill: {e:#}");
                    }
                }
                tracing::info!("backfill recibido: {n} snapshots");
            }
            Ok(AgentMessage::Response { req_id, .. }) | Ok(AgentMessage::LogChunk { req_id, .. }) => {
                // Las solicitudes bajo demanda llegan en la fase 3
                tracing::debug!("respuesta {req_id} sin solicitud pendiente");
            }
            Ok(AgentMessage::Hello { .. }) => tracing::warn!("Hello repetido ignorado"),
            Err(e) => tracing::warn!("mensaje inválido del agente: {e}"),
        }
    }
}

fn describe(runtimes: &[RuntimeDescriptor]) -> String {
    if runtimes.is_empty() {
        return "ninguno".into();
    }
    runtimes
        .iter()
        .map(|r| format!("{:?} {}", r.kind, r.version.as_deref().unwrap_or("?")))
        .collect::<Vec<_>>()
        .join(", ")
}
