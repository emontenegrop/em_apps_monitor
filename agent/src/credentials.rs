use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use uuid::Uuid;

/// Credencial del agente (`/etc/em-agent/credentials.toml`, permisos 0600)
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub server_id: Uuid,
    pub secret: String,
}

impl Credentials {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("no se pudo leer la credencial {}", path.display()))?;
        warn_if_readable_by_others(path);
        toml::from_str(&raw).with_context(|| format!("credencial inválida en {}", path.display()))
    }

    /// Valor de la cabecera `Authorization`
    pub fn bearer(&self) -> String {
        format!("Bearer {}.{}", self.server_id, self.secret)
    }
}

#[cfg(unix)]
fn warn_if_readable_by_others(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.permissions().mode() & 0o077 != 0 {
            tracing::warn!("{} es legible por otros usuarios; usa chmod 600", path.display());
        }
    }
}

#[cfg(not(unix))]
fn warn_if_readable_by_others(_path: &Path) {}
