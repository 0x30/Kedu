use std::collections::HashMap;
use std::io;
use std::process::Command;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::{ApplicationIdentity, ApplicationNetworkRates, Pid, positive_delta};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkCounters {
    pub bytes_in: u64,
    pub bytes_out: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct NetworkRates {
    pub download_bytes_per_second: f64,
    pub upload_bytes_per_second: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessNetworkRow {
    pub pid: Pid,
    pub counters: NetworkCounters,
}

pub struct NetworkCollector {
    previous_frame: HashMap<Pid, NetworkCounters>,
    previous_frame_time: Option<Instant>,
}

impl Default for NetworkCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkCollector {
    pub fn new() -> Self {
        Self {
            previous_frame: HashMap::new(),
            previous_frame_time: None,
        }
    }

    pub fn stop(&mut self) {
        self.previous_frame.clear();
        self.previous_frame_time = None;
    }

    /// Captures and calculates one network frame.
    ///
    /// The mutable receiver intentionally serializes `nettop` invocations for each collector.
    pub fn sample_rates(
        &mut self,
        identities_by_pid: &HashMap<Pid, ApplicationIdentity>,
    ) -> io::Result<ApplicationNetworkRates> {
        let current_frame = self.capture_process_counters()?;
        let now = Instant::now();
        let previous_frame = std::mem::replace(&mut self.previous_frame, current_frame.clone());
        let previous_frame_time = self.previous_frame_time.replace(now);
        let Some(previous_frame_time) = previous_frame_time else {
            return Ok(ApplicationNetworkRates::default());
        };
        let elapsed = now.duration_since(previous_frame_time).as_secs_f64();
        if elapsed <= 0.0 {
            return Ok(ApplicationNetworkRates::default());
        }

        let mut result = ApplicationNetworkRates::default();
        for (pid, current) in current_frame {
            let (Some(previous), Some(identity)) =
                (previous_frame.get(&pid), identities_by_pid.get(&pid))
            else {
                continue;
            };
            let process_rates = rates(current, *previous, elapsed);
            result.by_pid.insert(pid, process_rates);
            let application_rates = result.totals.entry(identity.id.clone()).or_default();
            application_rates.download_bytes_per_second += process_rates.download_bytes_per_second;
            application_rates.upload_bytes_per_second += process_rates.upload_bytes_per_second;
        }
        Ok(result)
    }

    pub fn capture_process_counters(&mut self) -> io::Result<HashMap<Pid, NetworkCounters>> {
        let output = Command::new("/usr/bin/nettop")
            .args(["-P", "-L", "1", "-n", "-x", "-J", "bytes_in,bytes_out"])
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "nettop exited with status {}",
                output.status
            )));
        }

        let output = String::from_utf8_lossy(&output.stdout);
        Ok(output
            .lines()
            .filter_map(parse_process_row)
            .map(|row| (row.pid, row.counters))
            .collect())
    }
}

pub fn parse_process_row(line: &str) -> Option<ProcessNetworkRow> {
    if line.is_empty() || line.starts_with(',') {
        return None;
    }
    let mut columns = line.split(',');
    let process = columns.next()?;
    let pid = process.rsplit_once('.')?.1.parse().ok()?;
    let bytes_in = columns.next()?.parse().ok()?;
    let bytes_out = columns.next()?.parse().ok()?;
    Some(ProcessNetworkRow {
        pid,
        counters: NetworkCounters {
            bytes_in,
            bytes_out,
        },
    })
}

pub fn rates(
    current: NetworkCounters,
    previous: NetworkCounters,
    elapsed_seconds: f64,
) -> NetworkRates {
    if elapsed_seconds <= 0.0 || !elapsed_seconds.is_finite() {
        return NetworkRates::default();
    }
    NetworkRates {
        download_bytes_per_second: positive_delta(current.bytes_in, previous.bytes_in) as f64
            / elapsed_seconds,
        upload_bytes_per_second: positive_delta(current.bytes_out, previous.bytes_out) as f64
            / elapsed_seconds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nettop_process_row() {
        let row =
            parse_process_row("Google Chrome H.34675,176921561,312224,").expect("valid nettop row");

        assert_eq!(row.pid, 34675);
        assert_eq!(
            row.counters,
            NetworkCounters {
                bytes_in: 176_921_561,
                bytes_out: 312_224,
            }
        );
    }

    #[test]
    fn ignores_headers_and_malformed_rows() {
        assert_eq!(parse_process_row(",bytes_in,bytes_out,"), None);
        assert_eq!(parse_process_row("no-pid,1,2,"), None);
        assert_eq!(parse_process_row("name.42,not-a-number,2,"), None);
    }

    #[test]
    fn calculates_rates_from_cumulative_counters() {
        let result = rates(
            NetworkCounters {
                bytes_in: 4_000,
                bytes_out: 900,
            },
            NetworkCounters {
                bytes_in: 1_000,
                bytes_out: 400,
            },
            2.0,
        );

        assert_eq!(result.download_bytes_per_second, 1_500.0);
        assert_eq!(result.upload_bytes_per_second, 250.0);
    }

    #[test]
    fn counter_reset_and_invalid_elapsed_produce_zero() {
        let reset = rates(
            NetworkCounters {
                bytes_in: 10,
                bytes_out: 10,
            },
            NetworkCounters {
                bytes_in: 20,
                bytes_out: 20,
            },
            1.0,
        );
        assert_eq!(reset, NetworkRates::default());
        assert_eq!(
            rates(NetworkCounters::default(), NetworkCounters::default(), 0.0),
            NetworkRates::default()
        );
    }
}
