use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// URL o host registrado para monitorear
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Target {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub description: Option<String>,
    pub monitor_type: String,
    pub is_active: bool,
    pub check_interval_secs: i32,
    pub timeout_secs: i32,
    pub expected_status_codes: String,
    pub alert_threshold: i32,
    pub consecutive_failures: i32,
    pub last_status: Option<String>,
    pub last_check_at: Option<DateTime<Utc>>,
    pub last_response_ms: Option<f64>,
    /// Último estado notificado por correo (offline | online)
    pub notified_status: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Una verificación guardada
#[derive(Debug, Clone, Serialize)]
pub struct CheckRecord {
    pub target_id: Uuid,
    pub time: DateTime<Utc>,
    pub is_success: bool,
    pub response_ms: Option<f64>,
    pub error_message: Option<String>,
    pub status_code: Option<i32>,
    pub ssl_valid: Option<bool>,
    pub ssl_expiry_days: Option<i32>,
    pub redirect_url: Option<String>,
    pub packet_loss: Option<f64>,
    pub ttl: Option<i32>,
    /// Estado del objetivo después de esta verificación
    pub status: String,
    pub consecutive_failures: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateChange {
    /// Alcanzó el umbral de fallos consecutivos
    ToOffline,
    /// Volvió a responder después de estar fuera de línea
    ToOnline,
}

/// Nuevo estado tras una verificación: (estado, fallos consecutivos).
/// Un fallo deja `warning` hasta llegar al umbral de fallos consecutivos; ahí pasa a `offline`.
pub fn transition(prev_failures: i32, threshold: i32, success: bool) -> (&'static str, i32) {
    if success {
        return ("online", 0);
    }
    let failures = prev_failures + 1;
    if failures >= threshold.max(1) {
        ("offline", failures)
    } else {
        ("warning", failures)
    }
}

/// Alerta pendiente según el estado actual y el último notificado. Una caída se notifica
/// una vez; la recuperación, solo si antes se notificó la caída. Mientras no se entregue
/// (notificaciones desactivadas, sin destinatarios o error SMTP) sigue pendiente.
pub fn pending_alert(status: &str, notified: Option<&str>) -> Option<StateChange> {
    match (status, notified) {
        ("offline", n) if n != Some("offline") => Some(StateChange::ToOffline),
        ("online", Some("offline")) => Some(StateChange::ToOnline),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_one_goes_offline_on_first_failure() {
        assert_eq!(transition(0, 1, false), ("offline", 1));
        assert_eq!(transition(1, 1, false), ("offline", 2));
    }

    #[test]
    fn warning_until_threshold() {
        assert_eq!(transition(0, 3, false), ("warning", 1));
        assert_eq!(transition(1, 3, false), ("warning", 2));
        assert_eq!(transition(2, 3, false), ("offline", 3));
        assert_eq!(transition(5, 3, true), ("online", 0));
    }

    #[test]
    fn offline_alert_pending_until_delivered() {
        assert_eq!(pending_alert("offline", None), Some(StateChange::ToOffline));
        assert_eq!(pending_alert("offline", Some("online")), Some(StateChange::ToOffline));
        assert_eq!(pending_alert("offline", Some("offline")), None);
    }

    #[test]
    fn recovery_only_after_notified_outage() {
        assert_eq!(pending_alert("online", Some("offline")), Some(StateChange::ToOnline));
        assert_eq!(pending_alert("online", None), None);
        assert_eq!(pending_alert("online", Some("online")), None);
        assert_eq!(pending_alert("warning", Some("offline")), None);
    }
}
