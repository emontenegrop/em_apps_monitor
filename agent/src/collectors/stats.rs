use std::{collections::HashMap, time::Instant};

use chrono::{DateTime, Utc};

use bollard::{
    container::{ListContainersOptions, MemoryStatsStats, Stats, StatsOptions},
    Docker,
};
use em_protocol::metrics::ContainerMetrics;
use futures_util::{stream, StreamExt};
use uuid::Uuid;

use super::{percent, rate_per_sec};

/// Contadores acumulados de un contenedor en un instante
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// ns de CPU consumidos por el contenedor desde que arrancó
    pub cpu_total: u64,
    pub net_rx: u64,
    pub net_tx: u64,
    pub blk_read: u64,
    pub blk_write: u64,
    /// Marca de tiempo de la stat según el runtime (campo `read`)
    pub read: Option<DateTime<Utc>>,
    /// Momento local en que llegó la respuesta (respaldo si `read` falta)
    pub at: Instant,
}

/// Segundos entre dos muestras, preferentemente según el reloj del runtime
pub fn elapsed_secs(prev: &Sample, cur: &Sample) -> f64 {
    match (prev.read, cur.read) {
        (Some(a), Some(b)) if b > a => (b - a).num_nanoseconds().unwrap_or(0) as f64 / 1e9,
        _ => cur.at.saturating_duration_since(prev.at).as_secs_f64(),
    }
}

/// % de CPU entre dos muestras (100 % = una CPU completa, como `docker stats`).
///
/// Se calcula contra el tiempo real y no con `system_cpu_usage`, porque Podman
/// no lo reporta con la misma semántica que Docker (no lo multiplica por el número de CPUs).
/// `None` si los contadores retrocedieron (reinicio del contenedor) o no pasó tiempo.
pub fn cpu_percent(prev: &Sample, cur: &Sample, secs: f64) -> Option<f64> {
    let cpu_delta = cur.cpu_total.checked_sub(prev.cpu_total)?;
    (secs > 0.0).then(|| cpu_delta as f64 / (secs * 1e9) * 100.0)
}

/// Bytes/s entre dos contadores acumulados; `None` si el contador se reinició
pub fn counter_rate(prev: u64, cur: u64, secs: f64) -> Option<i64> {
    rate_per_sec(cur.checked_sub(prev)?, secs)
}

/// Memoria en uso sin la caché de páginas inactivas, igual que `docker stats`
fn memory_used(stats: &Stats) -> u64 {
    let usage = stats.memory_stats.usage.unwrap_or(0);
    let inactive = match stats.memory_stats.stats {
        Some(MemoryStatsStats::V1(v1)) => v1.total_inactive_file,
        Some(MemoryStatsStats::V2(v2)) => v2.inactive_file,
        None => 0,
    };
    usage.saturating_sub(inactive)
}

fn to_sample(stats: &Stats, at: Instant) -> Sample {
    // Docker usa la fecha cero ("0001-01-01T00:00:00Z") cuando no hay lectura
    let read = DateTime::parse_from_rfc3339(&stats.read)
        .ok()
        .map(|d| d.with_timezone(&Utc))
        .filter(|d| d.timestamp() > 0);
    let (net_rx, net_tx) = stats
        .networks
        .as_ref()
        .map(|nets| nets.values().fold((0, 0), |acc, n| (acc.0 + n.rx_bytes, acc.1 + n.tx_bytes)))
        .unwrap_or((0, 0));
    let (blk_read, blk_write) = stats
        .blkio_stats
        .io_service_bytes_recursive
        .as_deref()
        .map(|entries| {
            entries.iter().fold((0, 0), |acc, e| match e.op.to_ascii_lowercase().as_str() {
                "read" => (acc.0 + e.value, acc.1),
                "write" => (acc.0, acc.1 + e.value),
                _ => acc,
            })
        })
        .unwrap_or((0, 0));

    Sample {
        cpu_total: stats.cpu_stats.cpu_usage.total_usage,
        net_rx,
        net_tx,
        blk_read,
        blk_write,
        read,
        at,
    }
}

/// Muestra y stats crudas de un contenedor, o el error del runtime
type StatsResult = Result<(Sample, Stats), String>;

/// Recolector de stats de un runtime. Guarda la muestra anterior de cada
/// contenedor para calcular CPU y tasas por diferencia entre ciclos.
#[derive(Default)]
pub struct StatsCollector {
    previous: HashMap<String, Sample>,
    /// Errores del último ciclo (contenedores cuyas stats no se pudieron leer)
    pub failures: Vec<String>,
}

impl StatsCollector {
    pub async fn collect(&mut self, docker: &Docker, concurrency: usize) -> anyhow::Result<Vec<ContainerMetrics>> {
        let running = docker
            .list_containers(Some(ListContainersOptions::<String> {
                filters: HashMap::from([("status".to_string(), vec!["running".to_string()])]),
                ..Default::default()
            }))
            .await?;
        let ids: Vec<String> = running.into_iter().filter_map(|c| c.id).filter(|id| !id.is_empty()).collect();

        let fetched: Vec<(String, StatsResult)> = stream::iter(ids)
            .map(|id| async move {
                let mut s = docker.stats(&id, Some(StatsOptions { stream: false, one_shot: true }));
                let res = match s.next().await {
                    Some(Ok(st)) => Ok((to_sample(&st, Instant::now()), st)),
                    Some(Err(e)) => Err(e.to_string()),
                    None => Err("el runtime no devolvió stats".to_string()),
                };
                (id, res)
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        self.failures.clear();
        let mut current = HashMap::with_capacity(fetched.len());
        let mut result = Vec::with_capacity(fetched.len());

        for (id, res) in fetched {
            let (sample, stats) = match res {
                Ok(v) => v,
                Err(e) => {
                    tracing::debug!("stats de {id} fallaron: {e}");
                    self.failures.push(e);
                    continue;
                }
            };
            let prev = self.previous.get(&id);
            let secs = prev.map(|p| elapsed_secs(p, &sample)).unwrap_or(0.0);

            let mem_used = memory_used(&stats) as i64;
            let mem_limit = stats.memory_stats.limit.unwrap_or(0) as i64;

            result.push(ContainerMetrics {
                time: chrono::Utc::now(),
                container_id: id.clone(),
                server_id: Uuid::nil(),
                cpu_usage_percent: prev.and_then(|p| cpu_percent(p, &sample, secs)),
                memory_used_bytes: Some(mem_used),
                memory_limit_bytes: Some(mem_limit),
                memory_usage_percent: Some(percent(mem_used as f64, mem_limit as f64)),
                net_rx_bytes: prev.and_then(|p| counter_rate(p.net_rx, sample.net_rx, secs)),
                net_tx_bytes: prev.and_then(|p| counter_rate(p.net_tx, sample.net_tx, secs)),
                block_read_bytes: prev.and_then(|p| counter_rate(p.blk_read, sample.blk_read, secs)),
                block_write_bytes: prev.and_then(|p| counter_rate(p.blk_write, sample.blk_write, secs)),
                pids: stats.pids_stats.current.map(|v| v as i64),
            });
            current.insert(id, sample);
        }

        // Se descartan las muestras de contenedores que ya no están corriendo
        self.previous = current;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn sample(cpu_total: u64, read_ms: Option<i64>, at: Instant) -> Sample {
        Sample {
            cpu_total,
            net_rx: 0,
            net_tx: 0,
            blk_read: 0,
            blk_write: 0,
            read: read_ms.and_then(DateTime::from_timestamp_millis),
            at,
        }
    }

    #[test]
    fn cpu_percent_against_wall_clock() {
        let t0 = Instant::now();
        // 1 s de CPU en 2 s reales = 50 % (media CPU)
        let a = sample(1_000_000_000, Some(10_000), t0);
        let b = sample(2_000_000_000, Some(12_000), t0);
        let pct = cpu_percent(&a, &b, elapsed_secs(&a, &b)).unwrap();
        assert!((pct - 50.0).abs() < 1e-9, "{pct}");

        // 2 CPUs saturadas durante 1 s = 200 %
        let c = sample(4_000_000_000, Some(13_000), t0);
        let pct = cpu_percent(&b, &c, elapsed_secs(&b, &c)).unwrap();
        assert!((pct - 200.0).abs() < 1e-9, "{pct}");
    }

    #[test]
    fn elapsed_falls_back_to_local_clock() {
        let t0 = Instant::now();
        let a = sample(0, None, t0);
        let b = sample(0, None, t0 + Duration::from_millis(1500));
        assert!((elapsed_secs(&a, &b) - 1.5).abs() < 1e-9);
    }

    #[test]
    fn cpu_percent_none_on_restart_or_no_progress() {
        let t0 = Instant::now();
        assert_eq!(cpu_percent(&sample(5_000, None, t0), &sample(100, None, t0), 1.0), None);
        assert_eq!(cpu_percent(&sample(1_000, None, t0), &sample(2_000, None, t0), 0.0), None);
    }

    #[test]
    fn counter_rate_per_second() {
        assert_eq!(counter_rate(1_000, 31_000, 15.0), Some(2_000));
        assert_eq!(counter_rate(31_000, 1_000, 15.0), None);
        assert_eq!(counter_rate(0, 100, 0.0), None);
    }
}
