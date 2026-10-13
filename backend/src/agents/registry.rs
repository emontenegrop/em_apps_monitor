use std::collections::HashMap;

use em_protocol::messages::{CentralMessage, CLOSE_REPLACED};
use tokio::sync::{mpsc, RwLock};
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

/// Conexiones de agentes vivas en esta instancia del backend
#[derive(Default)]
pub struct AgentRegistry {
    conns: RwLock<HashMap<Uuid, AgentHandle>>,
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
    pub async fn unregister(&self, server_id: Uuid, conn_id: Uuid) -> bool {
        let mut conns = self.conns.write().await;
        if conns.get(&server_id).is_some_and(|h| h.conn_id == conn_id) {
            conns.remove(&server_id);
            true
        } else {
            false
        }
    }

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
}
