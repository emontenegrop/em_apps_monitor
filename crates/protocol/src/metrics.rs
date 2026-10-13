use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct HostMetrics {
    pub time: DateTime<Utc>,
    pub server_id: Uuid,
    pub cpu_usage_percent: Option<f64>,
    pub memory_used_bytes: Option<i64>,
    pub memory_total_bytes: Option<i64>,
    pub memory_usage_percent: Option<f64>,
    pub swap_used_bytes: Option<i64>,
    pub swap_total_bytes: Option<i64>,
    pub net_rx_bytes: Option<i64>,
    pub net_tx_bytes: Option<i64>,
    pub load_avg_1: Option<f64>,
    pub load_avg_5: Option<f64>,
    pub load_avg_15: Option<f64>,
    pub process_count: Option<i32>,
    pub uptime_seconds: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct ContainerMetrics {
    pub time: DateTime<Utc>,
    pub container_id: String,
    pub server_id: Uuid,
    pub cpu_usage_percent: Option<f64>,
    pub memory_used_bytes: Option<i64>,
    pub memory_limit_bytes: Option<i64>,
    pub memory_usage_percent: Option<f64>,
    pub net_rx_bytes: Option<i64>,
    pub net_tx_bytes: Option<i64>,
    pub block_read_bytes: Option<i64>,
    pub block_write_bytes: Option<i64>,
    pub pids: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct DockerInfo {
    pub server_id: Uuid,
    pub docker_version: Option<String>,
    pub api_version: Option<String>,
    pub os_type: Option<String>,
    pub arch: Option<String>,
    pub kernel_version: Option<String>,
    pub containers_total: Option<i32>,
    pub containers_running: Option<i32>,
    pub containers_paused: Option<i32>,
    pub containers_stopped: Option<i32>,
    pub images_total: Option<i32>,
    pub storage_driver: Option<String>,
    pub runtime: Option<String>,
    pub docker_root_dir: Option<String>,
    pub swarm_active: Option<bool>,
    pub raw_info: Option<serde_json::Value>,
    pub updated_at: DateTime<Utc>,
}

/// Disk usage snapshot (not stored per-metric, returned on-demand)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiskUsage {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub usage_percent: f64,
    pub fs_type: String,
}
