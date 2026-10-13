pub mod disks;
pub mod host;
pub mod runtime;
pub mod stats;

/// Convierte un delta en bytes/s; `None` si el intervalo no es válido
pub fn rate_per_sec(delta: u64, secs: f64) -> Option<i64> {
    (secs > 0.0).then(|| (delta as f64 / secs).round() as i64)
}

pub fn percent(part: f64, total: f64) -> f64 {
    if total > 0.0 {
        part / total * 100.0
    } else {
        0.0
    }
}
