//! Conexión persistente agente → central por WebSocket.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use em_protocol::{
    messages::{
        AgentMessage, CentralMessage, RequestMethod, RequestResult, RuntimeDescriptor, Snapshot,
        CLOSE_DUPLICATE, CLOSE_INCOMPATIBLE_PROTOCOL, CLOSE_REPLACED, CLOSE_REVOKED,
    },
    PROTOCOL_VERSION,
};
use futures_util::{SinkExt, StreamExt};
use sysinfo::System;
use tokio::{sync::watch, time::Instant};
use tokio_tungstenite::{
    connect_async_tls_with_config,
    tungstenite::{
        self,
        client::IntoClientRequest,
        http::{header, HeaderValue, StatusCode},
        protocol::WebSocketConfig,
        Message,
    },
    Connector, MaybeTlsStream, WebSocketStream,
};

use crate::{
    buffer::SnapshotBuffer,
    collectors::runtime,
    config::{AgentConfig, RuntimeEndpoint},
    credentials::Credentials,
    snapshot::Collector,
    tls,
};

const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
/// Tamaño objetivo de cada lote de backfill (por debajo del máximo del central)
const BACKFILL_CHUNK_BYTES: usize = 3 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const WELCOME_TIMEOUT: Duration = Duration::from_secs(10);
const PING_EVERY: Duration = Duration::from_secs(20);
/// Sin tráfico del central durante este tiempo, la conexión se da por muerta
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(60);
/// Espera ante errores que no se arreglan reintentando (credencial rechazada, protocolo viejo)
const BACKOFF_FATAL: Duration = Duration::from_secs(600);
/// Espera tras ser reemplazado por otra conexión con la misma credencial
const BACKOFF_REPLACED: Duration = Duration::from_secs(60);
const REDISCOVER_EVERY: Duration = Duration::from_secs(60);

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// Estado compartido entre la tarea de recolección y la de conexión
struct Shared {
    buffer: SnapshotBuffer,
    /// La próxima recolección debe incluir la lista completa de contenedores
    force_containers: AtomicBool,
    interval: watch::Sender<u64>,
    runtimes: Mutex<Vec<RuntimeDescriptor>>,
}

/// Cómo terminó una sesión
enum SessionEnd {
    /// El central cerró la conexión con este código
    Closed(Option<u16>),
}

#[derive(Debug, thiserror::Error)]
enum ConnectError {
    #[error("el central rechazó la credencial (HTTP 401)")]
    Unauthorized,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// URL del WebSocket a partir de `server_url`
fn ws_url(cfg: &AgentConfig) -> Result<String> {
    let base = cfg
        .server_url
        .as_deref()
        .ok_or_else(|| anyhow!("falta server_url en la configuración"))?
        .trim_end_matches('/');
    if let Some(rest) = base.strip_prefix("https://") {
        Ok(format!("wss://{rest}/api/agent/ws"))
    } else if let Some(rest) = base.strip_prefix("http://") {
        if !cfg.allow_insecure_http {
            bail!("server_url usa http://: la credencial viajaría sin cifrar (allow_insecure_http = true solo para desarrollo)");
        }
        tracing::warn!("conexión SIN TLS al central: usar solo en desarrollo");
        Ok(format!("ws://{rest}/api/agent/ws"))
    } else {
        bail!("server_url debe empezar con https://")
    }
}

/// Ejecuta el agente: recolección periódica y conexión con reconexión automática.
pub async fn run(cfg: AgentConfig, creds: Credentials, collector: Collector) -> Result<()> {
    let url = ws_url(&cfg)?;
    let connector = if url.starts_with("wss://") {
        Some(Connector::Rustls(tls::client_config(&cfg.ca_file)?))
    } else {
        Some(Connector::Plain)
    };

    let shared = Arc::new(Shared {
        buffer: SnapshotBuffer::new(cfg.buffer_max_snapshots),
        force_containers: AtomicBool::new(true),
        interval: watch::Sender::new(cfg.collection_interval_secs),
        runtimes: Mutex::new(collector.runtimes()),
    });

    tokio::spawn(collect_loop(collector, shared.clone(), cfg.runtimes.clone()));

    tracing::info!("agente iniciado: servidor {} → {url}", creds.server_id);
    let mut backoff = BACKOFF_MIN;
    loop {
        let started = Instant::now();
        let wait = match session(&url, connector.clone(), &creds, &shared).await {
            Ok(SessionEnd::Closed(Some(CLOSE_INCOMPATIBLE_PROTOCOL))) => {
                tracing::error!("el central no acepta la versión de protocolo {PROTOCOL_VERSION}: actualiza el agente");
                BACKOFF_FATAL
            }
            Ok(SessionEnd::Closed(Some(CLOSE_REVOKED))) => {
                tracing::error!("el central revocó la credencial de este agente");
                BACKOFF_FATAL
            }
            Ok(SessionEnd::Closed(Some(CLOSE_DUPLICATE))) => {
                tracing::error!(
                    "la credencial {} ya la usa un agente en otro host: genera una credencial propia para este host",
                    cfg.credentials_file.display()
                );
                BACKOFF_FATAL
            }
            Ok(SessionEnd::Closed(Some(CLOSE_REPLACED))) => {
                tracing::error!("otra conexión con esta misma credencial reemplazó a esta (¿dos agentes en el mismo host?)");
                BACKOFF_REPLACED
            }
            Ok(SessionEnd::Closed(code)) => {
                tracing::warn!("el central cerró la conexión (código {code:?})");
                BACKOFF_MIN
            }
            Err(ConnectError::Unauthorized) => {
                tracing::error!("el central rechazó la credencial: revisa {}", cfg.credentials_file.display());
                BACKOFF_FATAL
            }
            Err(ConnectError::Other(e)) => {
                tracing::warn!("conexión con el central: {e:#}");
                // Una sesión que duró se considera sana: se reinicia la espera
                if started.elapsed() > BACKOFF_MAX {
                    backoff = BACKOFF_MIN;
                }
                let wait = backoff;
                backoff = (backoff * 2).min(BACKOFF_MAX);
                wait
            }
        };
        // Jitter de ±20 % para que muchos agentes no reconecten a la vez
        let jitter = wait.mul_f64(0.8 + fastrand::f64() * 0.4);
        tracing::info!(
            "reintento en {:.0}s ({} snapshots en espera)",
            jitter.as_secs_f64(),
            shared.buffer.len()
        );
        tokio::time::sleep(jitter).await;
    }
}

/// Recolecta snapshots al ritmo indicado por el central, haya o no conexión
async fn collect_loop(mut collector: Collector, shared: Arc<Shared>, configured: Vec<RuntimeEndpoint>) {
    let mut interval = shared.interval.subscribe();
    let mut last_discovery = Instant::now();
    loop {
        let secs = *interval.borrow_and_update();
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(secs)) => {}
            // Cambió el intervalo: se reprograma la espera
            _ = interval.changed() => continue,
        }

        // Si al arrancar no había runtimes (p. ej. Docker aún no iniciaba), se reintenta
        if collector.runtime_count() == 0 && last_discovery.elapsed() >= REDISCOVER_EVERY {
            last_discovery = Instant::now();
            let (clients, _) = runtime::discover(&configured).await;
            if !clients.is_empty() {
                tracing::info!("runtimes detectados: {}", clients.len());
                collector.replace_runtimes(clients);
                *shared.runtimes.lock().unwrap() = collector.runtimes();
                shared.force_containers.store(true, Ordering::Relaxed);
            }
        }

        let force = shared.force_containers.swap(false, Ordering::Relaxed);
        let snap = collector.snapshot(force).await;
        shared.buffer.push(snap);
    }
}

fn hello(shared: &Shared) -> AgentMessage {
    AgentMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
        hostname: System::host_name().unwrap_or_else(|| "desconocido".into()),
        os: System::long_os_version(),
        arch: std::env::consts::ARCH.to_string(),
        kernel: System::kernel_version(),
        runtimes: shared.runtimes.lock().unwrap().clone(),
    }
}

async fn send_json(sink: &mut futures_util::stream::SplitSink<Ws, Message>, msg: &AgentMessage) -> Result<()> {
    sink.send(Message::Text(serde_json::to_string(msg)?)).await.map_err(|e| anyhow!("{e}"))
}

async fn session(
    url: &str,
    connector: Option<Connector>,
    creds: &Credentials,
    shared: &Shared,
) -> Result<SessionEnd, ConnectError> {
    let mut req = url.into_client_request().context("URL inválida")?;
    req.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&creds.bearer()).context("credencial con caracteres inválidos")?,
    );
    req.headers_mut().insert(
        header::USER_AGENT,
        HeaderValue::from_static(concat!("em-agent/", env!("CARGO_PKG_VERSION"))),
    );

    let ws_config = WebSocketConfig {
        max_message_size: Some(MAX_MESSAGE_BYTES),
        max_frame_size: Some(MAX_MESSAGE_BYTES),
        ..Default::default()
    };
    let connect = connect_async_tls_with_config(req, Some(ws_config), true, connector);
    let (ws, _) = match tokio::time::timeout(CONNECT_TIMEOUT, connect).await {
        Err(_) => return Err(anyhow!("tiempo de conexión agotado").into()),
        Ok(Err(tungstenite::Error::Http(resp))) if resp.status() == StatusCode::UNAUTHORIZED => {
            return Err(ConnectError::Unauthorized)
        }
        // El Display de tungstenite ya incluye la causa; se aplana para no repetirla en el log
        Ok(Err(e)) => return Err(anyhow!("no se pudo conectar: {e}").into()),
        Ok(Ok(v)) => v,
    };
    let (mut sink, mut stream) = ws.split();

    // ── Hello / Welcome ──────────────────────────────────────────────────
    send_json(&mut sink, &hello(shared)).await?;
    // El central puede rechazar la sesión con un cierre antes del Welcome (p. ej. 4009);
    // ese código decide la espera antes de reintentar, igual que un cierre posterior
    let welcome = tokio::time::timeout(WELCOME_TIMEOUT, async {
        while let Some(msg) = stream.next().await {
            match msg.map_err(|e| anyhow!("{e}"))? {
                Message::Text(text) => return Ok(Ok(serde_json::from_str::<CentralMessage>(&text)?)),
                Message::Close(frame) => return Ok(Err(frame.map(|f| u16::from(f.code)))),
                _ => {}
            }
        }
        Err(anyhow!("conexión cerrada antes del Welcome"))
    })
    .await
    .map_err(|_| anyhow!("el central no envió Welcome"))??;

    let welcome = match welcome {
        Ok(msg) => msg,
        Err(code) => return Ok(SessionEnd::Closed(code)),
    };
    let CentralMessage::Welcome { server_id, collection_interval_secs, min_protocol_version } = welcome else {
        return Err(anyhow!("se esperaba Welcome").into());
    };
    if PROTOCOL_VERSION < min_protocol_version {
        return Ok(SessionEnd::Closed(Some(CLOSE_INCOMPATIBLE_PROTOCOL)));
    }
    if server_id != creds.server_id {
        tracing::warn!("el central identifica a este agente como {server_id}, la credencial dice {}", creds.server_id);
    }
    shared.interval.send_if_modified(|v| {
        let new = collection_interval_secs.max(5);
        std::mem::replace(v, new) != new
    });
    shared.force_containers.store(true, Ordering::Relaxed);
    tracing::info!("conectado al central (intervalo {collection_interval_secs}s)");

    send_pending(&mut sink, shared).await?;

    // ── Bucle principal ──────────────────────────────────────────────────
    let mut ping = tokio::time::interval(PING_EVERY);
    let mut last_rx = Instant::now();
    loop {
        tokio::select! {
            _ = shared.buffer.notify.notified() => send_pending(&mut sink, shared).await?,
            _ = ping.tick() => {
                if last_rx.elapsed() > IDLE_TIMEOUT {
                    return Err(anyhow!("sin respuesta del central en {}s", IDLE_TIMEOUT.as_secs()).into());
                }
                sink.send(Message::Ping(Vec::new())).await.map_err(|e| anyhow!("{e}"))?;
            }
            msg = stream.next() => {
                let msg = match msg {
                    None => return Ok(SessionEnd::Closed(None)),
                    Some(Err(e)) => return Err(anyhow!("{e}").into()),
                    Some(Ok(m)) => m,
                };
                last_rx = Instant::now();
                match msg {
                    Message::Text(text) => {
                        if let Some(reply) = handle_central(&text, shared) {
                            send_json(&mut sink, &reply).await?;
                        }
                    }
                    Message::Close(frame) => {
                        return Ok(SessionEnd::Closed(frame.map(|f| u16::from(f.code))));
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Procesa un mensaje del central y devuelve la respuesta, si corresponde
fn handle_central(text: &str, shared: &Shared) -> Option<AgentMessage> {
    match serde_json::from_str::<CentralMessage>(text) {
        Ok(CentralMessage::Request { req_id, method }) => {
            let result = match method {
                RequestMethod::Ping => RequestResult::Ok(serde_json::json!({ "pong": true })),
                // Las demás solicitudes bajo demanda llegan en la fase 3
                other => RequestResult::Error(format!("operación no soportada todavía: {other:?}")),
            };
            Some(AgentMessage::Response { req_id, result })
        }
        Ok(CentralMessage::ConfigUpdate { collection_interval_secs, log_level }) => {
            if let Some(secs) = collection_interval_secs {
                let _ = shared.interval.send(secs.max(5));
                tracing::info!("nuevo intervalo de recolección: {secs}s");
            }
            if let Some(level) = log_level {
                tracing::info!("cambio de nivel de log a {level} aún no soportado");
            }
            None
        }
        Ok(CentralMessage::Welcome { .. }) => None,
        Err(e) => {
            tracing::warn!("mensaje inválido del central: {e}");
            None
        }
    }
}

/// Envía todo lo pendiente: lo viejo como Backfill en lotes y lo último como Snapshot.
/// Si algo falla, lo que no se envió vuelve al buffer.
async fn send_pending(sink: &mut futures_util::stream::SplitSink<Ws, Message>, shared: &Shared) -> Result<()> {
    let mut pending = shared.buffer.drain();
    let Some(latest) = pending.pop() else { return Ok(()) };

    if !pending.is_empty() {
        tracing::info!("enviando backfill de {} snapshots", pending.len());
    }
    let mut rest = pending.into_iter().peekable();
    while rest.peek().is_some() {
        let mut chunk: Vec<Snapshot> = Vec::new();
        let mut size = 0;
        while let Some(next) = rest.peek() {
            let len = serde_json::to_vec(next).map(|v| v.len()).unwrap_or(0);
            if !chunk.is_empty() && size + len > BACKFILL_CHUNK_BYTES {
                break;
            }
            size += len;
            chunk.push(rest.next().unwrap());
        }
        let msg = AgentMessage::Backfill { snapshots: chunk };
        if let Err(e) = send_json(sink, &msg).await {
            let AgentMessage::Backfill { snapshots } = msg else { unreachable!() };
            shared.buffer.requeue(snapshots.into_iter().chain(rest).chain([latest]).collect());
            return Err(e);
        }
    }

    let msg = AgentMessage::Snapshot(latest);
    if let Err(e) = send_json(sink, &msg).await {
        let AgentMessage::Snapshot(latest) = msg else { unreachable!() };
        shared.buffer.requeue(vec![latest]);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(url: &str, insecure: bool) -> AgentConfig {
        AgentConfig { server_url: Some(url.into()), allow_insecure_http: insecure, ..Default::default() }
    }

    #[test]
    fn builds_websocket_url() {
        assert_eq!(
            ws_url(&cfg("https://172.16.27.31:8443/", false)).unwrap(),
            "wss://172.16.27.31:8443/api/agent/ws"
        );
        assert_eq!(ws_url(&cfg("http://localhost:3000", true)).unwrap(), "ws://localhost:3000/api/agent/ws");
    }

    #[test]
    fn rejects_plain_http_unless_allowed() {
        assert!(ws_url(&cfg("http://localhost:3000", false)).is_err());
        assert!(ws_url(&cfg("ftp://x", true)).is_err());
        assert!(ws_url(&AgentConfig::default()).is_err());
    }
}
