use std::time::Instant;

use em_protocol::metrics::HostMetrics;
use sysinfo::{Networks, System};
use uuid::Uuid;

use super::rate_per_sec;

/// Interfaces que no representan tráfico real del host (loopback y redes de contenedores)
fn is_virtual_iface(name: &str) -> bool {
    name == "lo"
        || name == "docker0"
        || ["veth", "br-", "cni", "podman", "virbr", "flannel", "cali", "vxlan"]
            .iter()
            .any(|p| name.starts_with(p))
}

/// Recolector de métricas del host. Mantiene estado entre ciclos para que
/// CPU y red sean deltas reales y no acumulados desde el arranque.
pub struct HostCollector {
    sys: System,
    networks: Networks,
    last_net_refresh: Instant,
}

impl HostCollector {
    pub fn new() -> Self {
        let mut sys = System::new();
        // Línea base: la primera lectura de CPU de sysinfo siempre es 0
        sys.refresh_cpu();
        Self {
            sys,
            networks: Networks::new_with_refreshed_list(),
            last_net_refresh: Instant::now(),
        }
    }

    pub fn collect(&mut self) -> HostMetrics {
        self.sys.refresh_cpu();
        self.sys.refresh_memory();
        self.sys.refresh_processes();

        self.networks.refresh();
        let elapsed = self.last_net_refresh.elapsed().as_secs_f64();
        self.last_net_refresh = Instant::now();
        let (rx, tx) = self
            .networks
            .iter()
            .filter(|(name, _)| !is_virtual_iface(name))
            .fold((0u64, 0u64), |acc, (_, d)| (acc.0 + d.received(), acc.1 + d.transmitted()));

        let memory_total = self.sys.total_memory() as i64;
        let memory_used = self.sys.used_memory() as i64;
        let load = System::load_average();

        HostMetrics {
            time: chrono::Utc::now(),
            // El central sustituye este valor por el server_id autenticado
            server_id: Uuid::nil(),
            cpu_usage_percent: Some(self.sys.global_cpu_info().cpu_usage() as f64),
            memory_used_bytes: Some(memory_used),
            memory_total_bytes: Some(memory_total),
            memory_usage_percent: Some(super::percent(memory_used as f64, memory_total as f64)),
            swap_used_bytes: Some(self.sys.used_swap() as i64),
            swap_total_bytes: Some(self.sys.total_swap() as i64),
            net_rx_bytes: rate_per_sec(rx, elapsed),
            net_tx_bytes: rate_per_sec(tx, elapsed),
            load_avg_1: Some(load.one),
            load_avg_5: Some(load.five),
            load_avg_15: Some(load.fifteen),
            process_count: Some(self.sys.processes().len() as i32),
            uptime_seconds: Some(System::uptime() as i64),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_virtual_iface;

    #[test]
    fn filters_container_interfaces() {
        for name in ["lo", "docker0", "veth12ab", "br-3f2a", "podman0", "cni0"] {
            assert!(is_virtual_iface(name), "{name}");
        }
        for name in ["eth0", "ens192", "enp3s0", "wlan0", "bond0"] {
            assert!(!is_virtual_iface(name), "{name}");
        }
    }
}
