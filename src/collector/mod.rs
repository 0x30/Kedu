mod network;
mod process;

use std::borrow::Borrow;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use network::{NetworkCollector, NetworkCounters, NetworkRates, ProcessNetworkRow};
pub use process::{ProcessCollector, app_root_path, cpu_percent, positive_delta};

pub type Pid = i32;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ApplicationIdentity {
    pub id: String,
    pub name: String,
    pub bundle_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessSample {
    pub pid: Pid,
    pub name: String,
    pub cpu_percent: f64,
    pub memory_bytes: u64,
    pub disk_read_bytes_per_second: f64,
    pub disk_write_bytes_per_second: f64,
    pub network_download_bytes_per_second: f64,
    pub network_upload_bytes_per_second: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplicationSample {
    pub identity: ApplicationIdentity,
    pub processes: Vec<ProcessSample>,
    pub cpu_percent: f64,
    pub memory_bytes: u64,
    pub disk_read_bytes_per_second: f64,
    pub disk_write_bytes_per_second: f64,
    pub network_download_bytes_per_second: f64,
    pub network_upload_bytes_per_second: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessSnapshot {
    pub timestamp_unix_ms: i64,
    pub applications: Vec<ApplicationSample>,
}

impl ProcessSnapshot {
    pub fn total_cpu_percent(&self) -> f64 {
        self.applications.iter().map(|app| app.cpu_percent).sum()
    }

    pub fn system_cpu_percent(&self) -> f64 {
        let logical_cpus = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
            .max(1);
        (self.total_cpu_percent() / logical_cpus as f64).clamp(0.0, 100.0)
    }

    pub fn total_memory_bytes(&self) -> u64 {
        self.applications.iter().map(|app| app.memory_bytes).sum()
    }

    pub fn merge_network(&mut self, rates: impl Borrow<ApplicationNetworkRates>) {
        let rates = rates.borrow();
        for application in &mut self.applications {
            let app_rates = rates
                .totals
                .get(&application.identity.id)
                .copied()
                .unwrap_or_default();
            application.network_download_bytes_per_second = app_rates.download_bytes_per_second;
            application.network_upload_bytes_per_second = app_rates.upload_bytes_per_second;

            for process in &mut application.processes {
                let process_rates = rates.by_pid.get(&process.pid).copied().unwrap_or_default();
                process.network_download_bytes_per_second = process_rates.download_bytes_per_second;
                process.network_upload_bytes_per_second = process_rates.upload_bytes_per_second;
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApplicationNetworkRates {
    pub totals: HashMap<String, NetworkRates>,
    pub by_pid: HashMap<Pid, NetworkRates>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_network_rates_into_applications_and_processes() {
        let mut snapshot = ProcessSnapshot {
            timestamp_unix_ms: 1,
            applications: vec![ApplicationSample {
                identity: ApplicationIdentity {
                    id: "app:/Applications/Test.app".into(),
                    name: "Test".into(),
                    bundle_path: Some("/Applications/Test.app".into()),
                },
                processes: vec![ProcessSample {
                    pid: 42,
                    name: "Test".into(),
                    cpu_percent: 0.0,
                    memory_bytes: 1,
                    disk_read_bytes_per_second: 0.0,
                    disk_write_bytes_per_second: 0.0,
                    network_download_bytes_per_second: 0.0,
                    network_upload_bytes_per_second: 0.0,
                }],
                cpu_percent: 0.0,
                memory_bytes: 1,
                disk_read_bytes_per_second: 0.0,
                disk_write_bytes_per_second: 0.0,
                network_download_bytes_per_second: 0.0,
                network_upload_bytes_per_second: 0.0,
            }],
        };
        let rates = ApplicationNetworkRates {
            totals: HashMap::from([(
                "app:/Applications/Test.app".into(),
                NetworkRates {
                    download_bytes_per_second: 150.0,
                    upload_bytes_per_second: 25.0,
                },
            )]),
            by_pid: HashMap::from([(
                42,
                NetworkRates {
                    download_bytes_per_second: 100.0,
                    upload_bytes_per_second: 10.0,
                },
            )]),
        };

        snapshot.merge_network(&rates);

        assert_eq!(
            snapshot.applications[0].network_download_bytes_per_second,
            150.0
        );
        assert_eq!(
            snapshot.applications[0].processes[0].network_upload_bytes_per_second,
            10.0
        );
    }
}
