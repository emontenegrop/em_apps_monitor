//! Tipos compartidos entre `em-agent` y el backend de EM Monitor.
//!
//! - [`metrics`] y [`container`]: datos que recolecta el agente.
//! - [`messages`]: mensajes que viajan por el WebSocket agente ↔ central.

pub mod container;
pub mod messages;
pub mod metrics;

/// Versión del protocolo agente ↔ central. Se incrementa ante cambios incompatibles.
pub const PROTOCOL_VERSION: u32 = 1;
