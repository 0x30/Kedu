use std::{collections::VecDeque, time::Duration};

use crate::collector::ProcessSnapshot;

#[derive(Debug)]
pub struct HistoryStore {
    retention: Duration,
    maximum_samples: usize,
    snapshots: VecDeque<ProcessSnapshot>,
    latest: Option<ProcessSnapshot>,
}

impl HistoryStore {
    pub fn new(retention: Duration, maximum_samples: usize) -> Self {
        Self {
            retention,
            maximum_samples: maximum_samples.max(2),
            snapshots: VecDeque::new(),
            latest: None,
        }
    }

    pub fn append(&mut self, snapshot: ProcessSnapshot) {
        self.latest = Some(snapshot.clone());
        self.snapshots.push_back(compact(snapshot));
        self.trim();
    }

    pub fn snapshots(&self) -> Vec<ProcessSnapshot> {
        self.snapshots.iter().cloned().collect()
    }

    pub fn latest(&self) -> Option<ProcessSnapshot> {
        self.latest.clone()
    }

    fn trim(&mut self) {
        while self.snapshots.len() > self.maximum_samples {
            self.snapshots.pop_front();
        }

        let Some(latest) = self.snapshots.back() else {
            return;
        };
        let retention_ms = i64::try_from(self.retention.as_millis()).unwrap_or(i64::MAX);
        let cutoff = latest.timestamp_unix_ms.saturating_sub(retention_ms);
        while self
            .snapshots
            .front()
            .is_some_and(|snapshot| snapshot.timestamp_unix_ms < cutoff)
        {
            self.snapshots.pop_front();
        }
    }
}

fn compact(mut snapshot: ProcessSnapshot) -> ProcessSnapshot {
    for application in &mut snapshot.applications {
        application.processes.clear();
    }
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::{ApplicationIdentity, ApplicationSample, ProcessSample};

    fn snapshot(timestamp_unix_ms: i64) -> ProcessSnapshot {
        ProcessSnapshot {
            timestamp_unix_ms,
            applications: vec![ApplicationSample {
                identity: ApplicationIdentity {
                    id: "test".into(),
                    name: "Test".into(),
                    bundle_path: None,
                },
                processes: vec![ProcessSample {
                    pid: 42,
                    name: "test".into(),
                    cpu_percent: 1.0,
                    memory_bytes: 2,
                    disk_read_bytes_per_second: 3.0,
                    disk_write_bytes_per_second: 4.0,
                    network_download_bytes_per_second: 5.0,
                    network_upload_bytes_per_second: 6.0,
                }],
                cpu_percent: 1.0,
                memory_bytes: 2,
                disk_read_bytes_per_second: 3.0,
                disk_write_bytes_per_second: 4.0,
                network_download_bytes_per_second: 5.0,
                network_upload_bytes_per_second: 6.0,
            }],
        }
    }

    #[test]
    fn keeps_full_latest_and_compacts_history() {
        let mut store = HistoryStore::new(Duration::from_secs(60), 10);
        store.append(snapshot(1_000));
        assert!(store.snapshots()[0].applications[0].processes.is_empty());
        assert_eq!(store.latest().unwrap().applications[0].processes.len(), 1);
    }

    #[test]
    fn trims_by_count_and_time() {
        let mut store = HistoryStore::new(Duration::from_secs(1), 2);
        store.append(snapshot(0));
        store.append(snapshot(1_000));
        store.append(snapshot(2_000));
        assert_eq!(store.snapshots().len(), 2);
        assert_eq!(store.snapshots()[0].timestamp_unix_ms, 1_000);
    }
}
