use std::{path::Path, sync::Arc};

use anyhow::{bail, Context, Result};
use rustls::{ClientConfig, RootCertStore};

/// Configuración TLS que solo confía en la CA interna del central
pub fn client_config(ca_file: &Path) -> Result<Arc<ClientConfig>> {
    let pem = std::fs::read(ca_file).with_context(|| format!("no se pudo leer la CA {}", ca_file.display()))?;
    let mut roots = RootCertStore::empty();
    for cert in rustls_pemfile::certs(&mut pem.as_slice()) {
        roots
            .add(cert.with_context(|| format!("PEM inválido en {}", ca_file.display()))?)
            .context("certificado de CA rechazado")?;
    }
    if roots.is_empty() {
        bail!("{} no contiene certificados", ca_file.display());
    }

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}
