use sqlx::PgPool;
use tokio::sync::broadcast;

use crate::{
    agents::{enroll::AgentCa, registry::AgentRegistry},
    auth::RateLimiter,
};

pub struct AppState {
    pub db: PgPool,
    /// Channel to broadcast real-time metric snapshots as JSON strings to WS clients
    pub broadcast_tx: broadcast::Sender<String>,
    /// Conexiones vivas de agentes remotos
    pub agents: AgentRegistry,
    /// Intervalo de recolección que se indica a los agentes en el Welcome
    pub collection_interval_secs: u64,
    pub login_limiter: RateLimiter,
    pub enroll_limiter: RateLimiter,
    pub cookie_secure: bool,
    pub agent_ca: Option<AgentCa>,
    pub public_url: Option<String>,
}

impl AppState {
    pub fn new(db: PgPool, cfg: &crate::config::Config, agent_ca: Option<AgentCa>) -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        Self {
            db,
            broadcast_tx,
            agents: AgentRegistry::default(),
            collection_interval_secs: cfg.collection_interval_secs,
            // 5 fallos por IP cada 15 min
            login_limiter: RateLimiter::new(5, std::time::Duration::from_secs(900)),
            enroll_limiter: RateLimiter::new(5, std::time::Duration::from_secs(900)),
            cookie_secure: cfg.cookie_secure,
            agent_ca,
            public_url: cfg.public_url.clone(),
        }
    }
}
