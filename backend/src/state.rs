use std::{collections::HashMap, sync::Arc};

use bollard::Docker;
use sqlx::PgPool;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::agents::registry::AgentRegistry;

pub struct AppState {
    pub db: PgPool,
    /// Channel to broadcast real-time metric snapshots as JSON strings to WS clients
    pub broadcast_tx: broadcast::Sender<String>,
    /// Cached Docker clients, keyed by server UUID
    pub docker_clients: RwLock<HashMap<Uuid, Arc<Docker>>>,
    /// Conexiones vivas de agentes remotos
    pub agents: AgentRegistry,
    /// Intervalo de recolección que se indica a los agentes en el Welcome
    pub collection_interval_secs: u64,
}

impl AppState {
    pub fn new(db: PgPool, collection_interval_secs: u64) -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        Self {
            db,
            broadcast_tx,
            docker_clients: RwLock::new(HashMap::new()),
            agents: AgentRegistry::default(),
            collection_interval_secs,
        }
    }
}
