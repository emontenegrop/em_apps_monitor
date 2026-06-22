use anyhow::Result;
use sysinfo::{CpuRefreshKind, Disks, Networks, System};
use uuid::Uuid;

use crate::models::metrics::{DiskUsage, HostMetrics};

/// Collect host metrics from the local machine via sysinfo 0.30.x.
/// Remote servers need a separate agent for host metrics.
pub fn collect_local_host_metrics(server_id: Uuid) -> Result<HostMetrics> {
    let mut sys = System::new_all();

    // Two-pass CPU: first call sets the baseline, second reads the delta
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_specifics(CpuRefreshKind::everything());

    let cpu_usage = sys.global_cpu_info().cpu_usage() as f64;

    sys.refresh_memory();
    let memory_total = sys.total_memory() as i64;
    let memory_used = sys.used_memory() as i64;
    let swap_total = sys.total_swap() as i64;
    let swap_used = sys.used_swap() as i64;
    let memory_pct = if memory_total > 0 {
        (memory_used as f64 / memory_total as f64) * 100.0
    } else {
        0.0
    };

    let load = System::load_average();
    let uptime = System::uptime() as i64;

    // sys.processes() populated by the initial new_all()
    let process_count = sys.processes().len() as i32;

    // Aggregate network traffic across all interfaces
    let networks = Networks::new_with_refreshed_list();
    let mut net_rx: i64 = 0;
    let mut net_tx: i64 = 0;
    for (_, data) in networks.iter() {
        net_rx += data.total_received() as i64;
        net_tx += data.total_transmitted() as i64;
    }

    Ok(HostMetrics {
        time: chrono::Utc::now(),
        server_id,
        cpu_usage_percent: Some(cpu_usage),
        memory_used_bytes: Some(memory_used),
        memory_total_bytes: Some(memory_total),
        memory_usage_percent: Some(memory_pct),
        swap_used_bytes: Some(swap_used),
        swap_total_bytes: Some(swap_total),
        net_rx_bytes: Some(net_rx),
        net_tx_bytes: Some(net_tx),
        load_avg_1: Some(load.one),
        load_avg_5: Some(load.five),
        load_avg_15: Some(load.fifteen),
        process_count: Some(process_count),
        uptime_seconds: Some(uptime),
    })
}

/// Collect disk usage from the local machine.
pub fn collect_disk_usage() -> Vec<DiskUsage> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .map(|d| {
            let total = d.total_space();
            let available = d.available_space();
            let used = total.saturating_sub(available);
            let pct = if total > 0 {
                (used as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            DiskUsage {
                name: d.name().to_string_lossy().to_string(),
                mount_point: d.mount_point().display().to_string(),
                total_bytes: total,
                used_bytes: used,
                available_bytes: available,
                usage_percent: pct,
                fs_type: d.file_system().to_string_lossy().to_string(),
            }
        })
        .collect()
}
