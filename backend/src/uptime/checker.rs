//! Verificación de un objetivo: HTTP(S) con validación de certificado, o ping ICMP.

use std::{
    net::IpAddr,
    time::{Duration, Instant},
};

use chrono::Utc;
use surge_ping::{Client, Config, IcmpPacket, PingIdentifier, PingSequence, ICMP};

/// Paquetes por verificación de ping
const PING_COUNT: u16 = 4;
const PING_GAP: Duration = Duration::from_millis(200);

/// Resultado de una verificación
#[derive(Debug, Default, Clone)]
pub struct CheckOutcome {
    pub is_success: bool,
    pub response_ms: Option<f64>,
    pub error_message: Option<String>,
    pub status_code: Option<i32>,
    pub ssl_valid: Option<bool>,
    pub ssl_expiry_days: Option<i32>,
    pub redirect_url: Option<String>,
    pub packet_loss: Option<f64>,
    pub ttl: Option<i32>,
}

/// Códigos HTTP aceptados ("200,301,302"). Vacío = cualquier 2xx o 3xx.
pub fn parse_expected_codes(s: &str) -> Vec<u16> {
    s.split(',').filter_map(|c| c.trim().parse::<u16>().ok()).collect()
}

pub fn status_ok(code: u16, expected: &[u16]) -> bool {
    if expected.is_empty() {
        (200..400).contains(&code)
    } else {
        expected.contains(&code)
    }
}

pub async fn check_http(url: &str, timeout: Duration, expected_codes: &str) -> CheckOutcome {
    let mut out = CheckOutcome::default();
    let client = match reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::limited(10))
        .tls_info(true)
        .user_agent(concat!("EM-Monitor/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            out.error_message = Some(format!("No se pudo crear el cliente HTTP: {e}"));
            return out;
        }
    };

    let https = url.to_ascii_lowercase().starts_with("https://");
    let started = Instant::now();
    match client.get(url).send().await {
        Ok(resp) => {
            out.response_ms = Some(round2(started.elapsed().as_secs_f64() * 1000.0));
            let code = resp.status().as_u16();
            out.status_code = Some(code as i32);
            out.is_success = status_ok(code, &parse_expected_codes(expected_codes));
            if !out.is_success {
                out.error_message = Some(format!("Código HTTP {code} no esperado"));
            }
            if resp.url().as_str() != url {
                out.redirect_url = Some(truncate(resp.url().as_str(), 512));
            }
            // Certificado del servidor que respondió (el último si hubo redirecciones)
            if resp.url().scheme() == "https" {
                let der = resp
                    .extensions()
                    .get::<reqwest::tls::TlsInfo>()
                    .and_then(|i| i.peer_certificate().map(|d| d.to_vec()));
                out.ssl_valid = Some(true);
                out.ssl_expiry_days = der.as_deref().and_then(cert_days_left);
            }
        }
        Err(e) => {
            out.error_message = Some(describe_reqwest_error(&e, timeout));
            if https && is_tls_error(&e) {
                out.ssl_valid = Some(false);
            }
        }
    }
    out
}

/// Días hasta el vencimiento de un certificado DER
fn cert_days_left(der: &[u8]) -> Option<i32> {
    let (_, cert) = x509_parser::parse_x509_certificate(der).ok()?;
    let not_after = cert.validity().not_after.timestamp();
    Some(((not_after - Utc::now().timestamp()) as f64 / 86_400.0).floor() as i32)
}

fn describe_reqwest_error(e: &reqwest::Error, timeout: Duration) -> String {
    if e.is_timeout() {
        return format!("Sin respuesta en {} s", timeout.as_secs());
    }
    // La causa raíz dice más que el envoltorio de reqwest ("error sending request")
    let mut msg = e.to_string();
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        msg = s.to_string();
        src = s.source();
    }
    if e.is_connect() {
        format!("Error de conexión: {msg}")
    } else {
        format!("Error: {msg}")
    }
}

fn is_tls_error(e: &reqwest::Error) -> bool {
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        let t = s.to_string().to_ascii_lowercase();
        if t.contains("certificate") || t.contains("tls") || t.contains("handshake") {
            return true;
        }
        src = s.source();
    }
    false
}

pub async fn check_ping(host: &str, timeout: Duration) -> CheckOutcome {
    let mut out = CheckOutcome { packet_loss: Some(100.0), ..Default::default() };

    let ip = match resolve(host).await {
        Ok(ip) => ip,
        Err(e) => {
            out.error_message = Some(e);
            return out;
        }
    };
    let config = match ip {
        IpAddr::V4(_) => Config::default(),
        IpAddr::V6(_) => Config::builder().kind(ICMP::V6).build(),
    };
    let client = match Client::new(&config) {
        Ok(c) => c,
        Err(e) => {
            out.error_message = Some(format!("No se pudo abrir el socket ICMP (¿falta NET_RAW?): {e}"));
            return out;
        }
    };

    // Identificador distinto por verificación: las respuestas se asocian por él
    let mut pinger = client.pinger(ip, PingIdentifier(rand::random())).await;
    pinger.timeout(timeout);
    let payload = [0u8; 56];
    let mut times = Vec::new();
    let mut ttl = None;
    let mut last_err = None;
    for seq in 0..PING_COUNT {
        match pinger.ping(PingSequence(seq), &payload).await {
            Ok((packet, rtt)) => {
                times.push(rtt.as_secs_f64() * 1000.0);
                if let IcmpPacket::V4(p) = packet {
                    ttl = ttl.or(p.get_ttl().map(i32::from));
                }
            }
            Err(e) => last_err = Some(e.to_string()),
        }
        if seq + 1 < PING_COUNT {
            tokio::time::sleep(PING_GAP).await;
        }
    }

    let ok = times.len() as f64;
    out.packet_loss = Some(round2((f64::from(PING_COUNT) - ok) / f64::from(PING_COUNT) * 100.0));
    if times.is_empty() {
        out.error_message = Some(match last_err {
            Some(e) => format!("Se perdieron los {PING_COUNT} paquetes ({e})"),
            None => format!("Se perdieron los {PING_COUNT} paquetes"),
        });
    } else {
        out.is_success = true;
        out.response_ms = Some(round2(times.iter().sum::<f64>() / ok));
        out.ttl = ttl;
    }
    out
}

async fn resolve(host: &str) -> Result<IpAddr, String> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(ip);
    }
    tokio::net::lookup_host((host, 0))
        .await
        .map_err(|e| format!("No se pudo resolver {host}: {e}"))?
        .map(|a| a.ip())
        .next()
        .ok_or_else(|| format!("{host} no tiene direcciones"))
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_expected_codes() {
        assert_eq!(parse_expected_codes("200, 301,abc,302"), vec![200, 301, 302]);
        assert!(parse_expected_codes("").is_empty());
    }

    #[test]
    fn status_check() {
        assert!(status_ok(200, &[200, 301]));
        assert!(!status_ok(404, &[200, 301]));
        assert!(status_ok(302, &[]));
        assert!(!status_ok(500, &[]));
    }
}
