use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub server_addr: String,
    pub collection_interval_secs: u64,
    pub _metrics_retention_days: i32,
    /// Recolector central vía bollard para servidores sin agente (transición; se retira en la fase 7)
    pub legacy_collector: bool,
    /// Administrador inicial (solo se usa si no hay usuarios)
    pub admin_username: Option<String>,
    pub admin_password: Option<String>,
    /// Cookie de sesión con `Secure` (desactivar solo en desarrollo sin HTTPS)
    pub cookie_secure: bool,
    /// CA interna que se entrega a los agentes al inscribirse
    pub agent_ca_file: String,
    /// URL pública del central para el comando de instalación, p. ej. https://172.16.27.31:8443
    pub public_url: Option<String>,
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
            admin_username: std::env::var("ADMIN_USERNAME").ok().filter(|v| !v.is_empty()),
            admin_password: std::env::var("ADMIN_PASSWORD").ok().filter(|v| !v.is_empty()),
            cookie_secure: std::env::var("COOKIE_SECURE")
                .map(|v| !matches!(v.to_ascii_lowercase().as_str(), "0" | "false" | "no"))
                .unwrap_or(true),
            agent_ca_file: std::env::var("AGENT_CA_FILE")
                .unwrap_or_else(|_| "/etc/em-monitor/ca.crt".to_string()),
            public_url: std::env::var("PUBLIC_URL")
                .ok()
                .filter(|v| !v.is_empty())
                .map(|v| v.trim_end_matches('/').to_string()),
        })
    }
}
