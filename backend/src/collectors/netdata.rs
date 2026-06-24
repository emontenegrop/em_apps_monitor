use std::collections::HashMap;

use anyhow::{anyhow, Result};
use reqwest::Client;
use uuid::Uuid;

use crate::models::metrics::HostMetrics;

pub struct NetdataClient {
    client: Client,
    base_url: String,
}

impl NetdataClient {
    pub fn new(base_url: &str) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Fetch a Netdata chart and return a map of dimension_name → latest_value.
    async fn fetch_chart(&self, chart: &str) -> Result<HashMap<String, f64>> {
        let url = format!(
            "{}/api/v1/data?chart={}&after=-30&points=1&format=json",
            self.base_url, chart
        );
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("Chart {chart}: HTTP {}", resp.status()));
        }
        let val: serde_json::Value = resp.json().await?;

        let dims = val["dimension_names"]
            .as_array()
            .ok_or_else(|| anyhow!("Chart {chart}: missing dimension_names"))?;
        let vals = val["latest_values"]
            .as_array()
            .ok_or_else(|| anyhow!("Chart {chart}: missing latest_values"))?;

        Ok(dims
            .iter()
            .zip(vals.iter())
            .filter_map(|(d, v)| Some((d.as_str()?.to_string(), v.as_f64()?)))
            .collect())
    }

    fn get(map: &HashMap<String, f64>, key: &str) -> f64 {
        map.get(key).copied().unwrap_or(0.0)
    }

    pub async fn collect_host_metrics(&self, server_id: Uuid) -> Result<HostMetrics> {
        // CPU — each dimension is % of total; sum = total used %
        let cpu = self.fetch_chart("system.cpu").await?;
        let cpu_usage = cpu.values().sum::<f64>();

        // RAM — Netdata reports in MiB
        let ram = self.fetch_chart("system.ram").await?;
        let mem_used_mib   = Self::get(&ram, "used");
        let mem_free_mib   = Self::get(&ram, "free");
        let mem_cached_mib = Self::get(&ram, "cached");
        let mem_buf_mib    = Self::get(&ram, "buffers");
        let mem_total_mib  = mem_used_mib + mem_free_mib + mem_cached_mib + mem_buf_mib;
        let mem_used_bytes  = mib_to_bytes(mem_used_mib);
        let mem_total_bytes = mib_to_bytes(mem_total_mib);
        let mem_pct = pct(mem_used_mib, mem_total_mib);

        // Swap — optional chart, not all hosts have swap
        let (swap_used_bytes, swap_total_bytes) = self
            .fetch_chart("mem.swap")
            .await
            .map(|s| {
                let used = Self::get(&s, "used");
                let free = Self::get(&s, "free");
                (mib_to_bytes(used), mib_to_bytes(used + free))
            })
            .unwrap_or((0, 0));

        // Load average
        let load = self.fetch_chart("system.load").await?;
        let load1  = Self::get(&load, "load1");
        let load5  = Self::get(&load, "load5");
        let load15 = Self::get(&load, "load15");

        // Uptime in seconds — optional chart
        let uptime_seconds = self
            .fetch_chart("system.uptime")
            .await
            .ok()
            .and_then(|u| u.into_values().next())
            .unwrap_or(0.0) as i64;

        // Processes — running + blocked as approximation of active processes
        let process_count = self
            .fetch_chart("system.processes")
            .await
            .ok()
            .map(|p| p.values().sum::<f64>() as i32)
            .unwrap_or(0);

        // Network — Netdata reports kbits/s; received is negative by convention
        let (net_rx_bytes, net_tx_bytes) = self
            .fetch_chart("system.net")
            .await
            .map(|n| {
                let rx = Self::get(&n, "received").abs();
                let tx = Self::get(&n, "sent").abs();
                (kbits_to_bytes(rx), kbits_to_bytes(tx))
            })
            .unwrap_or((0, 0));

        Ok(HostMetrics {
            time: chrono::Utc::now(),
            server_id,
            cpu_usage_percent: Some(cpu_usage),
            memory_used_bytes: Some(mem_used_bytes),
            memory_total_bytes: Some(mem_total_bytes),
            memory_usage_percent: Some(mem_pct),
            swap_used_bytes: Some(swap_used_bytes),
            swap_total_bytes: Some(swap_total_bytes),
            net_rx_bytes: Some(net_rx_bytes),
            net_tx_bytes: Some(net_tx_bytes),
            load_avg_1: Some(load1),
            load_avg_5: Some(load5),
            load_avg_15: Some(load15),
            process_count: Some(process_count),
            uptime_seconds: Some(uptime_seconds),
        })
    }
}

fn mib_to_bytes(mib: f64) -> i64 {
    (mib * 1024.0 * 1024.0) as i64
}

// kbits/s → bytes/s (instantaneous rate, not cumulative)
fn kbits_to_bytes(kbits: f64) -> i64 {
    (kbits * 1000.0 / 8.0) as i64
}

fn pct(part: f64, total: f64) -> f64 {
    if total > 0.0 {
        (part / total) * 100.0
    } else {
        0.0
    }
}
