use std::{collections::VecDeque, sync::Mutex};

use em_protocol::messages::Snapshot;
use tokio::sync::Notify;

/// Cola acotada de snapshots pendientes de enviar. Si se llena mientras no hay
/// conexión, se descartan los más viejos.
pub struct SnapshotBuffer {
    queue: Mutex<VecDeque<Snapshot>>,
    max: usize,
    pub notify: Notify,
}

impl SnapshotBuffer {
    pub fn new(max: usize) -> Self {
        Self { queue: Mutex::new(VecDeque::with_capacity(max)), max, notify: Notify::new() }
    }

    pub fn push(&self, snap: Snapshot) {
        let mut q = self.queue.lock().unwrap();
        if q.len() >= self.max {
            q.pop_front();
            tracing::warn!("buffer lleno ({}): se descarta el snapshot más viejo", self.max);
        }
        q.push_back(snap);
        drop(q);
        self.notify.notify_one();
    }

    /// Saca todos los snapshots pendientes, del más viejo al más nuevo
    pub fn drain(&self) -> Vec<Snapshot> {
        self.queue.lock().unwrap().drain(..).collect()
    }

    /// Devuelve snapshots que no se pudieron enviar al frente de la cola
    pub fn requeue(&self, snaps: Vec<Snapshot>) {
        let mut q = self.queue.lock().unwrap();
        for snap in snaps.into_iter().rev() {
            if q.len() >= self.max {
                break;
            }
            q.push_front(snap);
        }
    }

    pub fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use em_protocol::metrics::HostMetrics;

    use super::*;

    fn snap(seq: u64) -> Snapshot {
        Snapshot {
            seq,
            ts: chrono::Utc::now(),
            host: HostMetrics {
                time: chrono::Utc::now(),
                server_id: uuid::Uuid::nil(),
                cpu_usage_percent: None,
                memory_used_bytes: None,
                memory_total_bytes: None,
                memory_usage_percent: None,
                swap_used_bytes: None,
                swap_total_bytes: None,
                net_rx_bytes: None,
                net_tx_bytes: None,
                load_avg_1: None,
                load_avg_5: None,
                load_avg_15: None,
                process_count: None,
                uptime_seconds: None,
            },
            disks: vec![],
            runtimes: vec![],
        }
    }

    fn seqs(v: &[Snapshot]) -> Vec<u64> {
        v.iter().map(|s| s.seq).collect()
    }

    #[test]
    fn drops_oldest_when_full() {
        let b = SnapshotBuffer::new(3);
        (1..=5).for_each(|i| b.push(snap(i)));
        assert_eq!(seqs(&b.drain()), vec![3, 4, 5]);
        assert_eq!(b.len(), 0);
    }

    #[test]
    fn requeue_keeps_order_ahead_of_new_snapshots() {
        let b = SnapshotBuffer::new(10);
        b.push(snap(3));
        b.requeue(vec![snap(1), snap(2)]);
        assert_eq!(seqs(&b.drain()), vec![1, 2, 3]);
    }
}
