//! Alertas por correo (SMTP) cuando un objetivo cae o se recupera.
//! La configuración y los destinatarios se registran desde la UI; sin ellos no se envía nada.

use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, Context};
use chrono::{DateTime, FixedOffset, Utc};
use lettre::{
    message::{header::ContentType, Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use serde::Serialize;

use super::model::{CheckRecord, StateChange, Target};
use crate::state::AppState;

const SMTP_TIMEOUT: Duration = Duration::from_secs(30);
/// Hora de Ecuador (UTC−5, sin horario de verano) para los correos
const ECUADOR_OFFSET_SECS: i32 = -5 * 3600;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SmtpSettings {
    pub enabled: bool,
    pub smtp_host: String,
    pub smtp_port: i32,
    pub security: String,
    pub username: String,
    #[serde(skip)]
    pub password_enc: Option<String>,
    pub from_address: String,
    pub single_email_bcc: bool,
    pub updated_at: DateTime<Utc>,
}

pub async fn load_settings(db: &sqlx::PgPool) -> sqlx::Result<Option<SmtpSettings>> {
    sqlx::query_as::<_, SmtpSettings>("SELECT * FROM notification_settings WHERE id = 1")
        .fetch_optional(db)
        .await
}

pub async fn load_recipients(db: &sqlx::PgPool) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT email FROM notification_recipients ORDER BY email").fetch_all(db).await
}

fn transport(state: &AppState, s: &SmtpSettings) -> anyhow::Result<AsyncSmtpTransport<Tokio1Executor>> {
    let builder = match s.security.as_str() {
        "ssl" => AsyncSmtpTransport::<Tokio1Executor>::relay(&s.smtp_host)?,
        "starttls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&s.smtp_host)?,
        _ => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&s.smtp_host),
    };
    let mut builder = builder.port(s.smtp_port as u16).timeout(Some(SMTP_TIMEOUT));
    if !s.username.is_empty() {
        let password = match &s.password_enc {
            Some(enc) => state
                .settings_cipher
                .as_ref()
                .ok_or_else(|| anyhow!("el backend no tiene SETTINGS_KEY: no se puede leer la contraseña SMTP"))?
                .decrypt(enc)?,
            None => String::new(),
        };
        builder = builder.credentials(Credentials::new(s.username.clone(), password));
    }
    Ok(builder.build())
}

/// Envía el mismo correo a los destinatarios: uno por destinatario o uno solo con todos en CCO.
/// Devuelve cuántos correos se enviaron; el primer error corta el envío.
async fn deliver(
    state: &AppState,
    s: &SmtpSettings,
    recipients: &[String],
    subject: &str,
    text: String,
    html: String,
) -> anyhow::Result<usize> {
    if recipients.is_empty() {
        return Err(anyhow!("no hay destinatarios registrados"));
    }
    let from: Mailbox = s.from_address.parse().context("remitente inválido")?;
    let mailer = transport(state, s)?;
    let body = || MultiPart::alternative()
        .singlepart(SinglePart::builder().header(ContentType::TEXT_PLAIN).body(text.clone()))
        .singlepart(SinglePart::builder().header(ContentType::TEXT_HTML).body(html.clone()));

    if s.single_email_bcc {
        let mut b = Message::builder().from(from.clone()).to(from.clone()).subject(subject);
        for r in recipients {
            b = b.bcc(r.parse().with_context(|| format!("destinatario inválido: {r}"))?);
        }
        mailer.send(b.multipart(body())?).await?;
        return Ok(1);
    }

    let mut sent = 0;
    for r in recipients {
        let to: Mailbox = r.parse().with_context(|| format!("destinatario inválido: {r}"))?;
        let msg = Message::builder().from(from.clone()).to(to).subject(subject).multipart(body())?;
        mailer.send(msg).await.with_context(|| format!("enviando a {r}"))?;
        sent += 1;
    }
    Ok(sent)
}

/// Notifica una caída o recuperación. Devuelve `true` si se entregó; si no, la alerta queda
/// pendiente y se reintenta en la siguiente verificación (los errores van al log).
pub async fn send_state_alert(state: &Arc<AppState>, target: &Target, check: &CheckRecord, change: StateChange) -> bool {
    let settings = match load_settings(&state.db).await {
        Ok(Some(s)) if s.enabled => s,
        Ok(_) => {
            // Se reintenta en cada verificación: a nivel debug para no llenar el log
            tracing::debug!("disponibilidad: alerta de {} pendiente; notificaciones desactivadas o sin configurar", target.name);
            return false;
        }
        Err(e) => {
            tracing::warn!("disponibilidad: no se pudo leer la configuración SMTP: {e}");
            return false;
        }
    };
    let recipients = match load_recipients(&state.db).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("disponibilidad: no se pudieron leer los destinatarios: {e}");
            return false;
        }
    };
    if recipients.is_empty() {
        tracing::debug!("disponibilidad: alerta de {} pendiente; no hay destinatarios", target.name);
        return false;
    }

    let (subject, text, html) = match change {
        StateChange::ToOffline => (
            format!("🔴 Alerta: {} - Servicio caído", target.name),
            alert_text(target, check),
            alert_html(target, check),
        ),
        StateChange::ToOnline => (
            format!("✅ Recuperado: {} - Servicio restaurado", target.name),
            recovery_text(target, check),
            recovery_html(target, check),
        ),
    };
    match deliver(state, &settings, &recipients, &subject, text, html).await {
        Ok(n) => {
            tracing::info!("disponibilidad: alerta de {} enviada ({n} correo/s)", target.name);
            true
        }
        Err(e) => {
            tracing::warn!("disponibilidad: no se pudo enviar la alerta de {} (se reintentará): {e:#}", target.name);
            false
        }
    }
}

/// Correo de prueba con la configuración guardada (aunque las notificaciones estén desactivadas)
pub async fn send_test(state: &Arc<AppState>) -> anyhow::Result<usize> {
    let settings = load_settings(&state.db).await?.ok_or_else(|| anyhow!("primero guarda la configuración SMTP"))?;
    let recipients = load_recipients(&state.db).await?;
    let when = local_time(Utc::now());
    let text = format!(
        "Correo de prueba de EM Monitor.\n\nSi lo recibiste, la configuración SMTP funciona.\nEnviado: {when}\n"
    );
    let html = layout(
        "#2563eb",
        "✉️ Correo de prueba",
        &format!(
            "<p>Si recibiste este correo, la configuración SMTP de EM Monitor funciona.</p><p><strong>Enviado:</strong> {when}</p>"
        ),
    );
    deliver(state, &settings, &recipients, "Prueba de notificaciones - EM Monitor", text, html).await
}

// ── Plantillas ─────────────────────────────────────────────────────────────

fn local_time(t: DateTime<Utc>) -> String {
    let tz = FixedOffset::east_opt(ECUADOR_OFFSET_SECS).expect("offset válido");
    t.with_timezone(&tz).format("%Y-%m-%d %H:%M:%S (hora de Ecuador)").to_string()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn na<T: ToString>(v: Option<T>) -> String {
    v.map(|v| v.to_string()).unwrap_or_else(|| "N/A".into())
}

fn layout(color: &str, title: &str, content: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><body style="font-family:Arial,sans-serif;line-height:1.6;color:#333">
<div style="max-width:600px;margin:0 auto;padding:20px">
  <div style="background:{color};color:#fff;padding:20px;text-align:center;border-radius:5px 5px 0 0"><h2 style="margin:0">{title}</h2></div>
  <div style="background:#f8f9fa;padding:20px;border-radius:0 0 5px 5px">{content}</div>
  <div style="margin-top:20px;padding-top:20px;border-top:1px solid #ddd;font-size:12px;color:#666;text-align:center">
    <p>Monitoreo de disponibilidad - EM Monitor</p><p>Mensaje automático. No responder a este correo.</p>
  </div>
</div></body></html>"#
    )
}

fn row(k: &str, v: &str) -> String {
    format!(r#"<tr><td style="padding:8px;border-bottom:1px solid #ddd;font-weight:bold;width:40%">{k}</td><td style="padding:8px;border-bottom:1px solid #ddd">{v}</td></tr>"#)
}

fn kind_label(t: &Target) -> &'static str {
    if t.monitor_type == "ping" { "Ping" } else { "HTTP" }
}

fn alert_html(t: &Target, c: &CheckRecord) -> String {
    let url = esc(&t.url);
    let mut rows = vec![
        row("Servicio:", &esc(&t.name)),
        row("URL / host:", &if t.monitor_type == "http" { format!(r#"<a href="{url}">{url}</a>"#) } else { url.clone() }),
        row("Tipo:", kind_label(t)),
        row("Estado:", "<strong>FUERA DE LÍNEA</strong>"),
    ];
    if t.monitor_type == "http" {
        rows.push(row("Código HTTP:", &na(c.status_code)));
    } else {
        rows.push(row("Pérdida de paquetes:", &format!("{}%", na(c.packet_loss))));
    }
    rows.push(row("Tiempo de respuesta:", &format!("{} ms", na(c.response_ms))));
    rows.push(row("Fallos consecutivos:", &c.consecutive_failures.to_string()));
    rows.push(row("Última verificación:", &local_time(c.time)));
    let error = c
        .error_message
        .as_deref()
        .map(|e| format!(r#"<p style="margin-top:15px"><strong>Error:</strong> {}</p>"#, esc(e)))
        .unwrap_or_default();
    layout(
        "#dc3545",
        "🔴 Alerta de monitoreo",
        &format!(
            r#"<h3>Servicio caído: {}</h3><p>El siguiente servicio no está respondiendo correctamente:</p>
<table style="width:100%;border-collapse:collapse;margin-top:15px">{}</table>{error}
<p style="margin-top:20px"><strong>Acción requerida:</strong> verifique el estado del servicio y tome las medidas correctivas necesarias.</p>"#,
            esc(&t.name),
            rows.join("")
        ),
    )
}

fn alert_text(t: &Target, c: &CheckRecord) -> String {
    let detail = if t.monitor_type == "http" {
        format!("Código HTTP: {}", na(c.status_code))
    } else {
        format!("Pérdida de paquetes: {}%", na(c.packet_loss))
    };
    format!(
        "ALERTA DE MONITOREO\n\nServicio caído: {name}\n\nServicio: {name}\nURL / host: {url}\nTipo: {kind}\nEstado: FUERA DE LÍNEA\n{detail}\nTiempo de respuesta: {rt} ms\nFallos consecutivos: {f}\nÚltima verificación: {when}\n{err}\nAcción requerida: verifique el estado del servicio y tome las medidas correctivas necesarias.\n\n---\nMonitoreo de disponibilidad - EM Monitor\nMensaje automático. No responder a este correo.",
        name = t.name,
        url = t.url,
        kind = kind_label(t),
        rt = na(c.response_ms),
        f = c.consecutive_failures,
        when = local_time(c.time),
        err = c.error_message.as_deref().map(|e| format!("\nError: {e}\n")).unwrap_or_default(),
    )
}

fn recovery_html(t: &Target, c: &CheckRecord) -> String {
    let url = esc(&t.url);
    let mut rows = vec![
        row("URL / host:", &if t.monitor_type == "http" { format!(r#"<a href="{url}">{url}</a>"#) } else { url.clone() }),
    ];
    if t.monitor_type == "http" {
        rows.push(row("Código HTTP:", &na(c.status_code)));
    }
    rows.push(row("Tiempo de respuesta:", &format!("{} ms", na(c.response_ms))));
    rows.push(row("Recuperado:", &local_time(c.time)));
    layout(
        "#28a745",
        "✅ Servicio recuperado",
        &format!(
            r#"<h3>{}</h3><p>El servicio fue restaurado y responde correctamente:</p>
<table style="width:100%;border-collapse:collapse;margin-top:15px">{}</table>"#,
            esc(&t.name),
            rows.join("")
        ),
    )
}

fn recovery_text(t: &Target, c: &CheckRecord) -> String {
    format!(
        "SERVICIO RECUPERADO\n\n{name}\nURL / host: {url}\n{code}Tiempo de respuesta: {rt} ms\nRecuperado: {when}\n\n---\nMonitoreo de disponibilidad - EM Monitor",
        name = t.name,
        url = t.url,
        code = if t.monitor_type == "http" { format!("Código HTTP: {}\n", na(c.status_code)) } else { String::new() },
        rt = na(c.response_ms),
        when = local_time(c.time),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_html() {
        assert_eq!(esc(r#"<a href="x">&</a>"#), "&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;");
    }

    #[test]
    fn ecuador_time() {
        let t = DateTime::parse_from_rfc3339("2026-10-15T20:00:00Z").unwrap().with_timezone(&Utc);
        assert!(local_time(t).starts_with("2026-10-15 15:00:00"));
    }
}
