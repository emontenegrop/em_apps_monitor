use std::{collections::HashMap, sync::Mutex, time::Duration};

use em_protocol::messages::{CentralMessage, RequestMethod, RequestResult, CLOSE_REPLACED};
use tokio::sync::{mpsc, oneshot, RwLock};
use uuid::Uuid;

/// Lo que el central envía por la conexión de un agente
#[derive(Debug)]
pub enum Outbound {
    Message(CentralMessage),
    Close { code: u16, reason: &'static str },
}

struct AgentHandle {
    conn_id: Uuid,
    hostname: String,
    tx: mpsc::Sender<Outbound>,
}

struct Pending {
    server_id: Uuid,
    tx: oneshot::Sender<RequestResult>,
}

/// Error de una solicitud bajo demanda a un agente
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("el agente del servidor no está conectado")]
    Offline,
    #[error("el agente no respondió en {0}s")]
    Timeout(u64),
    #[error("{0}")]
    Agent(String),
}

/// Conexiones de agentes vivas en esta instancia del backend
#[derive(Default)]
pub struct AgentRegistry {
    conns: RwLock<HashMap<Uuid, AgentHandle>>,
    /// Solicitudes enviadas a agentes que esperan respuesta, por `req_id`
    pending: Mutex<HashMap<Uuid, Pending>>,
}

/// Quita la solicitud pendiente al terminar la espera (respuesta, timeout o cancelación)
struct PendingGuard<'a> {
    registry: &'a AgentRegistry,
    req_id: Uuid,
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        self.registry.pending.lock().unwrap().remove(&self.req_id);
    }
}

impl AgentRegistry {
    /// Registra la conexión. Si el mismo host ya tenía otra abierta (p. ej. un agente
    /// reiniciado con la conexión vieja medio abierta), la reemplaza.
    ///
    /// Si la conexión viva es de **otro** host, la credencial está duplicada (VM clonada,
    /// doble instalación): se rechaza la nueva y se devuelve el hostname que ya la usa.
    pub async fn register(
        &self,
        server_id: Uuid,
        conn_id: Uuid,
        hostname: &str,
        tx: mpsc::Sender<Outbound>,
    ) -> Result<(), String> {
        let mut conns = self.conns.write().await;
        if let Some(existing) = conns.get(&server_id) {
            if existing.hostname != hostname && !existing.tx.is_closed() {
                return Err(existing.hostname.clone());
            }
        }
        let old = conns.insert(server_id, AgentHandle { conn_id, hostname: hostname.to_string(), tx });
        if let Some(old) = old {
            let _ = old
                .tx
                .try_send(Outbound::Close { code: CLOSE_REPLACED, reason: "reemplazada por una conexión nueva" });
        }
        Ok(())
    }

    /// Quita la conexión solo si sigue siendo la registrada. Devuelve `true` si la quitó.
    /// Las solicitudes pendientes de ese agente fallan de inmediato como `Offline`.
    pub async fn unregister(&self, server_id: Uuid, conn_id: Uuid) -> bool {
        let mut conns = self.conns.write().await;
        if conns.get(&server_id).is_some_and(|h| h.conn_id == conn_id) {
            conns.remove(&server_id);
            self.pending.lock().unwrap().retain(|_, p| p.server_id != server_id);
            true
        } else {
            false
        }
    }

    /// Envía una solicitud al agente y espera su respuesta
    pub async fn request(
        &self,
        server_id: Uuid,
        method: RequestMethod,
        timeout: Duration,
    ) -> Result<serde_json::Value, RpcError> {
        let tx = self
            .conns
            .read()
            .await
            .get(&server_id)
            .map(|h| h.tx.clone())
            .ok_or(RpcError::Offline)?;

        let req_id = Uuid::new_v4();
        let (done_tx, done_rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(req_id, Pending { server_id, tx: done_tx });
        let _guard = PendingGuard { registry: self, req_id };

        tx.send(Outbound::Message(CentralMessage::Request { req_id, method }))
            .await
            .map_err(|_| RpcError::Offline)?;

        match tokio::time::timeout(timeout, done_rx).await {
            Err(_) => Err(RpcError::Timeout(timeout.as_secs())),
            // El agente se desconectó antes de responder
            Ok(Err(_)) => Err(RpcError::Offline),
            Ok(Ok(RequestResult::Ok(value))) => Ok(value),
            Ok(Ok(RequestResult::Error(msg))) => Err(RpcError::Agent(msg)),
        }
    }

    /// Entrega la respuesta de un agente. Solo se acepta si la solicitud fue enviada
    /// a ese mismo agente; devuelve `false` si no había nada pendiente.
    pub fn resolve(&self, server_id: Uuid, req_id: Uuid, result: RequestResult) -> bool {
        let mut pending = self.pending.lock().unwrap();
        match pending.get(&req_id) {
            Some(p) if p.server_id == server_id => {
                let p = pending.remove(&req_id).expect("presente");
                p.tx.send(result).is_ok()
            }
            _ => false,
        }
    }

    #[cfg(test)]
    pub async fn is_connected(&self, server_id: Uuid) -> bool {
        self.conns.read().await.contains_key(&server_id)
    }

    /// Cierra la conexión del agente (credenciales rotadas o revocadas)
    pub async fn disconnect(&self, server_id: Uuid, code: u16, reason: &'static str) {
        if let Some(h) = self.conns.read().await.get(&server_id) {
            let _ = h.tx.try_send(Outbound::Close { code, reason });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn same_host_replaces_and_other_host_is_rejected() {
        let reg = AgentRegistry::default();
        let sid = Uuid::new_v4();
        let (tx1, mut rx1) = mpsc::channel(4);
        let c1 = Uuid::new_v4();
        reg.register(sid, c1, "web01", tx1).await.unwrap();

        // Otro host con la misma credencial: rechazado, la conexión viva sigue
        let (tx2, _rx2) = mpsc::channel(4);
        assert_eq!(reg.register(sid, Uuid::new_v4(), "web02", tx2).await, Err("web01".into()));
        assert!(rx1.try_recv().is_err());

        // El mismo host reconecta: reemplaza y cierra la anterior con 4000
        let (tx3, _rx3) = mpsc::channel(4);
        let c3 = Uuid::new_v4();
        reg.register(sid, c3, "web01", tx3).await.unwrap();
        assert!(matches!(rx1.try_recv(), Ok(Outbound::Close { code: CLOSE_REPLACED, .. })));

        // La conexión vieja ya no puede quitar a la nueva
        assert!(!reg.unregister(sid, c1).await);
        assert!(reg.unregister(sid, c3).await);
        assert!(!reg.is_connected(sid).await);
    }

    /// Simula al agente: responde cada Request que recibe con `reply`
    fn fake_agent(
        reg: std::sync::Arc<AgentRegistry>,
        sid: Uuid,
        mut rx: mpsc::Receiver<Outbound>,
        reply: Option<RequestResult>,
    ) {
        tokio::spawn(async move {
            while let Some(Outbound::Message(CentralMessage::Request { req_id, .. })) = rx.recv().await {
                if let Some(r) = reply.clone() {
                    reg.resolve(sid, req_id, r);
                }
            }
        });
    }

    #[tokio::test]
    async fn request_roundtrip_and_errors() {
        let reg = std::sync::Arc::new(AgentRegistry::default());
        let sid = Uuid::new_v4();
        let t = Duration::from_millis(200);

        assert!(matches!(reg.request(sid, RequestMethod::Ping, t).await, Err(RpcError::Offline)));

        let (tx, rx) = mpsc::channel(4);
        reg.register(sid, Uuid::new_v4(), "h", tx).await.unwrap();
        fake_agent(reg.clone(), sid, rx, Some(RequestResult::Ok(serde_json::json!({"pong": true}))));
        assert_eq!(reg.request(sid, RequestMethod::Ping, t).await.unwrap()["pong"], true);
        assert!(reg.pending.lock().unwrap().is_empty());

        let sid2 = Uuid::new_v4();
        let (tx2, rx2) = mpsc::channel(4);
        reg.register(sid2, Uuid::new_v4(), "h2", tx2).await.unwrap();
        fake_agent(reg.clone(), sid2, rx2, Some(RequestResult::Error("no existe".into())));
        assert!(matches!(reg.request(sid2, RequestMethod::Ping, t).await, Err(RpcError::Agent(m)) if m == "no existe"));

        let sid3 = Uuid::new_v4();
        let (tx3, rx3) = mpsc::channel(4);
        reg.register(sid3, Uuid::new_v4(), "h3", tx3).await.unwrap();
        fake_agent(reg.clone(), sid3, rx3, None);
        assert!(matches!(reg.request(sid3, RequestMethod::Ping, t).await, Err(RpcError::Timeout(_))));
        // El timeout no deja la solicitud colgada
        assert!(reg.pending.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn response_from_another_agent_is_ignored() {
        let reg = AgentRegistry::default();
        let (victim, other) = (Uuid::new_v4(), Uuid::new_v4());
        let (done_tx, mut done_rx) = oneshot::channel();
        let req_id = Uuid::new_v4();
        reg.pending.lock().unwrap().insert(req_id, Pending { server_id: victim, tx: done_tx });

        assert!(!reg.resolve(other, req_id, RequestResult::Ok(serde_json::json!("falso"))));
        assert!(done_rx.try_recv().is_err());
        assert!(reg.resolve(victim, req_id, RequestResult::Ok(serde_json::json!("ok"))));
    }

    #[tokio::test]
    async fn disconnect_fails_pending_requests() {
        let reg = std::sync::Arc::new(AgentRegistry::default());
        let sid = Uuid::new_v4();
        let conn = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(4);
        reg.register(sid, conn, "h", tx).await.unwrap();

        let r = reg.clone();
        let call = tokio::spawn(async move { r.request(sid, RequestMethod::Ping, Duration::from_secs(5)).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        reg.unregister(sid, conn).await;
        assert!(matches!(call.await.unwrap(), Err(RpcError::Offline)));
    }
}
