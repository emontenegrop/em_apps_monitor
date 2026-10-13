use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub server_addr: String,
    pub collection_interval_secs: u64,
    pub _metrics_retention_days: i32,
    /// Recolector central vía bollard para servidores sin agente (transición; se retira en la fase 7)
    pub legacy_collector: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .context("DATABASE_URL must be set")?,
            server_addr: std::env::var("SERVER_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:3000".to_string()),
            collection_interval_secs: std::env::var("COLLECTION_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15),
            _metrics_retention_days: std::env::var("METRICS_RETENTION_DAYS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(7),
            legacy_collector: std::env::var("LEGACY_COLLECTOR")
                .map(|v| !matches!(v.to_ascii_lowercase().as_str(), "0" | "false" | "no"))
                .unwrap_or(true),
        })
    }
}
