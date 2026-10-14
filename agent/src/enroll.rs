//! `em-agent enroll`: inscribe el host en el central con una ficha de un solo uso.
//!
//! 1. Descarga la CA del central SIN verificar TLS (todavía no se conoce la CA).
//! 2. Compara su huella SHA-256 con la del comando de instalación: si no coincide, aborta.
//! 3. Canjea la ficha por la credencial, ya con TLS verificado contra esa CA.
//! 4. Guarda ca.crt, credentials.toml (0600) y em-agent.toml en el directorio de configuración.

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::CryptoProvider,
    pki_types::{CertificateDer, ServerName, UnixTime},
    DigitallySignedStruct, SignatureScheme,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::tls;

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

pub struct EnrollArgs {
    pub url: String,
    pub token: String,
    pub ca_fingerprint: String,
    pub config_dir: PathBuf,
}

#[derive(Deserialize)]
struct EnrollResponse {
    server_id: Uuid,
    secret: String,
}

/// Normaliza una huella: minúsculas, sin `:` ni espacios
pub fn normalize_fingerprint(fp: &str) -> Result<String> {
    let clean: String = fp.chars().filter(|c| *c != ':' && !c.is_whitespace()).collect::<String>().to_lowercase();
    if clean.len() != 64 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("huella de CA inválida: se esperan 64 caracteres hexadecimales (SHA-256)");
    }
    Ok(clean)
}

/// Huella SHA-256 del primer certificado del PEM
pub fn pem_fingerprint(pem: &[u8]) -> Result<String> {
    let der = rustls_pemfile::certs(&mut &pem[..])
        .next()
        .context("la respuesta no contiene ningún certificado")?
        .context("PEM inválido")?;
    Ok(hex::encode(Sha256::digest(der.as_ref())))
}

/// Acepta cualquier certificado. SOLO para descargar la CA, que luego se valida por huella.
#[derive(Debug)]
struct AcceptAnyCert(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn unverified_agent() -> Result<ureq::Agent> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCert(provider)))
        .with_no_client_auth();
    Ok(ureq::AgentBuilder::new().tls_config(Arc::new(config)).timeout(HTTP_TIMEOUT).build())
}

/// Escribe un archivo de forma atómica (temporal + rename) con los permisos indicados
fn write_file(path: &Path, contents: &[u8], mode: u32) -> Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(mode);
        }
        #[cfg(not(unix))]
        let _ = mode;
        let mut f = opts.open(&tmp).with_context(|| format!("no se pudo escribir {}", tmp.display()))?;
        f.write_all(contents)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path).with_context(|| format!("no se pudo escribir {}", path.display()))?;
    Ok(())
}

pub fn run(args: EnrollArgs) -> Result<()> {
    let url = args.url.trim_end_matches('/').to_string();
    if !url.starts_with("https://") {
        bail!("--url debe empezar con https://");
    }
    let expected = normalize_fingerprint(&args.ca_fingerprint)?;

    // 1–2. CA del central, validada por huella
    let ca_pem = unverified_agent()?
        .get(&format!("{url}/api/agent/ca.crt"))
        .call()
        .with_context(|| format!("no se pudo descargar la CA de {url}"))?
        .into_string()?;
    let actual = pem_fingerprint(ca_pem.as_bytes())?;
    if actual != expected {
        bail!(
            "la huella de la CA recibida no coincide con la del comando de instalación \
             (esperada {expected}, recibida {actual}). No se envió la ficha: verifica la URL; \
             podría haber un intermediario."
        );
    }
    println!("✔ CA del central verificada ({actual})");

    // 3. Canje de la ficha con TLS verificado contra esa CA
    let verified = ureq::AgentBuilder::new()
        .tls_config(tls::client_config_from_pem(ca_pem.as_bytes())?)
        .timeout(HTTP_TIMEOUT)
        .build();
    let hostname = sysinfo::System::host_name();
    let resp = verified
        .post(&format!("{url}/api/agent/enroll"))
        .send_json(serde_json::json!({ "token": args.token.trim(), "hostname": hostname }));
    let creds: EnrollResponse = match resp {
        Ok(r) => r.into_json()?,
        Err(ureq::Error::Status(401, _)) => bail!("el central rechazó la ficha: es inválida, ya se usó o venció"),
        Err(ureq::Error::Status(429, _)) => bail!("demasiados intentos fallidos desde este host; espera unos minutos"),
        Err(e) => return Err(e).context("no se pudo inscribir el agente"),
    };
    println!("✔ Agente inscrito como servidor {}", creds.server_id);

    // 4. Archivos de configuración
    let dir = &args.config_dir;
    std::fs::create_dir_all(dir).with_context(|| format!("no se pudo crear {}", dir.display()))?;
    let ca_path = dir.join("ca.crt");
    let creds_path = dir.join("credentials.toml");
    let cfg_path = dir.join("em-agent.toml");

    write_file(&ca_path, ca_pem.as_bytes(), 0o644)?;
    write_file(
        &creds_path,
        format!("server_id = \"{}\"\nsecret = \"{}\"\n", creds.server_id, creds.secret).as_bytes(),
        0o600,
    )?;

    if cfg_path.exists() {
        println!(
            "• {} ya existe: no se modificó. Verifica que server_url = \"{url}\"",
            cfg_path.display()
        );
    } else {
        let toml = format!(
            "server_url = \"{url}\"\nca_file = {:?}\ncredentials_file = {:?}\n",
            ca_path.display().to_string(),
            creds_path.display().to_string()
        );
        write_file(&cfg_path, toml.as_bytes(), 0o644)?;
    }
    println!("✔ Configuración guardada en {}", dir.display());
    println!("\nSiguiente paso: inicia el servicio (systemctl enable --now em-agent) o ejecuta `em-agent run`.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_fingerprints() {
        let fp = "4B:4E:DB:A4:25:B2:C3:70:CD:4C:E7:51:23:D4:83:6C:16:52:8E:FE:0A:A4:13:77:6A:21:63:81:88:79:4F:00";
        assert_eq!(
            normalize_fingerprint(fp).unwrap(),
            "4b4edba425b2c370cd4ce75123d4836c16528efe0aa413776a21638188794f00"
        );
        assert!(normalize_fingerprint("abc").is_err());
        assert!(normalize_fingerprint(&"z".repeat(64)).is_err());
    }

    #[test]
    fn fingerprint_matches_openssl() {
        let pem = include_bytes!("../../backend/tests/fixtures/test-ca.crt");
        let expected = include_str!("../../backend/tests/fixtures/test-ca.sha256").trim();
        assert_eq!(pem_fingerprint(pem).unwrap(), expected);
    }
}
