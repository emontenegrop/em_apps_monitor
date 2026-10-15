//! Monitoreo de disponibilidad de URLs (HTTP/HTTPS) e IPs (ping) desde el central,
//! con alertas por correo. Reemplaza a `qh-python-dash/url_monitor`.

pub mod api;
pub mod checker;
pub mod crypto;
pub mod model;
pub mod notify;
pub mod worker;
