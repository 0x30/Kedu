use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_int, c_uint, c_void};
use std::io;
use std::mem::{MaybeUninit, size_of};
use std::path::Path;
use std::ptr;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::{ApplicationIdentity, ApplicationSample, Pid, ProcessSample, ProcessSnapshot};

const RUSAGE_INFO_V4: c_int = 4;
const PROC_PIDTBSDINFO: c_int = 3;
const MAXCOMLEN: usize = 16;
const MAX_PATH_BUFFER: usize = 4096 * 4;

#[repr(C)]
#[derive(Clone, Copy)]
struct RUsageInfoV4 {
    uuid: [u8; 16],
    user_time: u64,
    system_time: u64,
    pkg_idle_wkups: u64,
    interrupt_wkups: u64,
    pageins: u64,
    wired_size: u64,
    resident_size: u64,
    phys_footprint: u64,
    proc_start_abstime: u64,
    proc_exit_abstime: u64,
    child_user_time: u64,
    child_system_time: u64,
    child_pkg_idle_wkups: u64,
    child_interrupt_wkups: u64,
    child_pageins: u64,
    child_elapsed_abstime: u64,
    diskio_bytesread: u64,
    diskio_byteswritten: u64,
    cpu_time_qos_default: u64,
    cpu_time_qos_maintenance: u64,
    cpu_time_qos_background: u64,
    cpu_time_qos_utility: u64,
    cpu_time_qos_legacy: u64,
    cpu_time_qos_user_initiated: u64,
    cpu_time_qos_user_interactive: u64,
    billed_system_time: u64,
    serviced_system_time: u64,
    logical_writes: u64,
    lifetime_max_phys_footprint: u64,
    instructions: u64,
    cycles: u64,
    billed_energy: u64,
    serviced_energy: u64,
    interval_max_phys_footprint: u64,
    runnable_time: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ProcBsdInfo {
    flags: u32,
    status: u32,
    xstatus: u32,
    pid: u32,
    ppid: u32,
    uid: u32,
    gid: u32,
    ruid: u32,
    rgid: u32,
    svuid: u32,
    svgid: u32,
    reserved: u32,
    command: [c_char; MAXCOMLEN],
    name: [c_char; 2 * MAXCOMLEN],
    nfiles: u32,
    pgid: u32,
    pjobc: u32,
    tty_device: u32,
    tty_pgid: u32,
    nice: i32,
    start_seconds: u64,
    start_microseconds: u64,
}

#[link(name = "proc")]
unsafe extern "C" {
    fn proc_listallpids(buffer: *mut c_void, buffer_size: c_int) -> c_int;
    fn proc_pid_rusage(pid: c_int, flavor: c_int, buffer: *mut c_void) -> c_int;
    fn proc_pidinfo(
        pid: c_int,
        flavor: c_int,
        argument: u64,
        buffer: *mut c_void,
        buffer_size: c_int,
    ) -> c_int;
    fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffer_size: c_uint) -> c_int;
    fn proc_name(pid: c_int, buffer: *mut c_void, buffer_size: c_uint) -> c_int;
}

#[derive(Debug, Clone, Copy)]
struct Counters {
    cpu_nanoseconds: u64,
    memory_bytes: u64,
    disk_read_bytes: u64,
    disk_write_bytes: u64,
    process_start_abstime: u64,
}

#[derive(Debug, Clone)]
struct ProcessRecord {
    pid: Pid,
    parent_pid: Pid,
    uid: u32,
    name: String,
    executable_path: Option<String>,
    counters: Counters,
}

#[derive(Default)]
struct Aggregate {
    processes: Vec<ProcessSample>,
    cpu_percent: f64,
    memory_bytes: u64,
    disk_read_bytes_per_second: f64,
    disk_write_bytes_per_second: f64,
}

pub struct ProcessCollector {
    previous_counters: HashMap<Pid, Counters>,
    previous_sample_time: Option<Instant>,
    latest_identities_by_pid: HashMap<Pid, ApplicationIdentity>,
    latest_parent_pids: HashMap<Pid, Pid>,
    latest_uids: HashMap<Pid, u32>,
    identity_cache: HashMap<String, ApplicationIdentity>,
}

impl Default for ProcessCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessCollector {
    pub fn new() -> Self {
        Self {
            previous_counters: HashMap::new(),
            previous_sample_time: None,
            latest_identities_by_pid: HashMap::new(),
            latest_parent_pids: HashMap::new(),
            latest_uids: HashMap::new(),
            identity_cache: HashMap::new(),
        }
    }

    pub fn sample(&mut self) -> io::Result<ProcessSnapshot> {
        let now = Instant::now();
        let elapsed = self
            .previous_sample_time
            .map(|previous| now.duration_since(previous).as_secs_f64().max(0.001));
        let records = self.process_records()?;
        let records_by_pid: HashMap<Pid, &ProcessRecord> =
            records.iter().map(|record| (record.pid, record)).collect();
        let mut aggregates: HashMap<ApplicationIdentity, Aggregate> = HashMap::new();
        let mut identities_by_pid = HashMap::with_capacity(records.len());
        let mut parent_pids = HashMap::with_capacity(records.len());
        let mut uids = HashMap::with_capacity(records.len());

        for record in &records {
            let identity = self.identity_for(record, &records_by_pid);
            identities_by_pid.insert(record.pid, identity.clone());
            parent_pids.insert(record.pid, record.parent_pid);
            uids.insert(record.pid, record.uid);

            let mut process_cpu_percent = 0.0;
            let mut disk_read_bytes_per_second = 0.0;
            let mut disk_write_bytes_per_second = 0.0;
            if let (Some(previous), Some(elapsed)) =
                (self.previous_counters.get(&record.pid), elapsed)
            {
                // A reused PID must establish a new baseline instead of inheriting the old process.
                if previous.process_start_abstime == record.counters.process_start_abstime {
                    process_cpu_percent = cpu_percent(
                        positive_delta(record.counters.cpu_nanoseconds, previous.cpu_nanoseconds),
                        elapsed,
                    );
                    disk_read_bytes_per_second =
                        positive_delta(record.counters.disk_read_bytes, previous.disk_read_bytes)
                            as f64
                            / elapsed;
                    disk_write_bytes_per_second =
                        positive_delta(record.counters.disk_write_bytes, previous.disk_write_bytes)
                            as f64
                            / elapsed;
                }
            }

            let aggregate = aggregates.entry(identity).or_default();
            aggregate.cpu_percent += process_cpu_percent;
            aggregate.memory_bytes = aggregate
                .memory_bytes
                .saturating_add(record.counters.memory_bytes);
            aggregate.disk_read_bytes_per_second += disk_read_bytes_per_second;
            aggregate.disk_write_bytes_per_second += disk_write_bytes_per_second;
            aggregate.processes.push(ProcessSample {
                pid: record.pid,
                name: record.name.clone(),
                cpu_percent: process_cpu_percent,
                memory_bytes: record.counters.memory_bytes,
                disk_read_bytes_per_second,
                disk_write_bytes_per_second,
                network_download_bytes_per_second: 0.0,
                network_upload_bytes_per_second: 0.0,
            });
        }

        self.previous_counters = records
            .iter()
            .map(|record| (record.pid, record.counters))
            .collect();
        self.previous_sample_time = Some(now);
        self.latest_identities_by_pid = identities_by_pid;
        self.latest_parent_pids = parent_pids;
        self.latest_uids = uids;

        let mut applications: Vec<_> = aggregates
            .into_iter()
            .map(|(identity, mut aggregate)| {
                aggregate.processes.sort_by(|left, right| {
                    right
                        .cpu_percent
                        .total_cmp(&left.cpu_percent)
                        .then_with(|| right.memory_bytes.cmp(&left.memory_bytes))
                        .then_with(|| left.pid.cmp(&right.pid))
                });
                ApplicationSample {
                    identity,
                    processes: aggregate.processes,
                    cpu_percent: aggregate.cpu_percent,
                    memory_bytes: aggregate.memory_bytes,
                    disk_read_bytes_per_second: aggregate.disk_read_bytes_per_second,
                    disk_write_bytes_per_second: aggregate.disk_write_bytes_per_second,
                    network_download_bytes_per_second: 0.0,
                    network_upload_bytes_per_second: 0.0,
                }
            })
            .collect();
        applications.sort_by(|left, right| {
            right
                .cpu_percent
                .total_cmp(&left.cpu_percent)
                .then_with(|| right.memory_bytes.cmp(&left.memory_bytes))
                .then_with(|| left.identity.name.cmp(&right.identity.name))
        });

        Ok(ProcessSnapshot {
            timestamp_unix_ms: unix_timestamp_millis(),
            applications,
        })
    }

    pub fn application_identities_by_pid(&self) -> &HashMap<Pid, ApplicationIdentity> {
        &self.latest_identities_by_pid
    }

    pub fn identities_by_pid(&self) -> HashMap<Pid, ApplicationIdentity> {
        self.latest_identities_by_pid.clone()
    }

    pub fn parent_pids(&self) -> &HashMap<Pid, Pid> {
        &self.latest_parent_pids
    }

    pub fn process_uids(&self) -> &HashMap<Pid, u32> {
        &self.latest_uids
    }

    fn process_records(&self) -> io::Result<Vec<ProcessRecord>> {
        let process_ids = all_process_ids()?;
        Ok(process_ids
            .into_iter()
            .filter_map(|pid| {
                let counters = counters_for(pid)?;
                let bsd_info = bsd_info_for(pid);
                let executable_path = executable_path_for(pid);
                let name = executable_path
                    .as_deref()
                    .and_then(|path| Path::new(path).file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .or_else(|| process_name_for(pid))
                    .unwrap_or_else(|| format!("PID {pid}"));
                Some(ProcessRecord {
                    pid,
                    parent_pid: bsd_info.map(|info| info.ppid as Pid).unwrap_or_default(),
                    uid: bsd_info.map(|info| info.uid).unwrap_or_default(),
                    name,
                    executable_path,
                    counters,
                })
            })
            .collect())
    }

    fn identity_for(
        &mut self,
        record: &ProcessRecord,
        records_by_pid: &HashMap<Pid, &ProcessRecord>,
    ) -> ApplicationIdentity {
        if let Some(root_path) = resolved_app_root(record, records_by_pid) {
            let key = format!("app:{root_path}");
            return self
                .identity_cache
                .entry(key.clone())
                .or_insert_with(|| ApplicationIdentity {
                    id: key,
                    name: Path::new(&root_path)
                        .file_stem()
                        .map(|name| name.to_string_lossy().into_owned())
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| record.name.clone()),
                    bundle_path: Some(root_path),
                })
                .clone();
        }

        let key = format!("process:{}", record.name);
        self.identity_cache
            .entry(key.clone())
            .or_insert_with(|| ApplicationIdentity {
                id: key,
                name: record.name.clone(),
                bundle_path: None,
            })
            .clone()
    }
}

pub fn app_root_path(executable_path: &str) -> Option<String> {
    if executable_path.ends_with(".app") {
        return Some(executable_path.to_owned());
    }
    executable_path
        .find(".app/")
        .map(|index| executable_path[..index + 4].to_owned())
}

pub fn positive_delta(current: u64, previous: u64) -> u64 {
    current.saturating_sub(previous)
}

pub fn cpu_percent(cpu_delta_nanoseconds: u64, elapsed_seconds: f64) -> f64 {
    if elapsed_seconds <= 0.0 || !elapsed_seconds.is_finite() {
        return 0.0;
    }
    cpu_delta_nanoseconds as f64 / 1_000_000_000.0 / elapsed_seconds * 100.0
}

fn resolved_app_root(
    record: &ProcessRecord,
    records_by_pid: &HashMap<Pid, &ProcessRecord>,
) -> Option<String> {
    let mut current = Some(record);
    let mut visited = HashSet::new();
    while let Some(candidate) = current {
        if !visited.insert(candidate.pid) {
            break;
        }
        if let Some(root_path) = candidate.executable_path.as_deref().and_then(app_root_path) {
            return Some(root_path);
        }
        if candidate.parent_pid <= 1 {
            break;
        }
        current = records_by_pid.get(&candidate.parent_pid).copied();
    }
    None
}

fn all_process_ids() -> io::Result<Vec<Pid>> {
    // SAFETY: A null buffer is explicitly supported and returns an estimated PID count.
    let estimated_count = unsafe { proc_listallpids(ptr::null_mut(), 0) };
    if estimated_count < 0 {
        return Err(io::Error::last_os_error());
    }
    let capacity = (estimated_count as usize).saturating_add(64).max(64);
    let mut process_ids = vec![0_i32; capacity];
    let buffer_size = process_ids
        .len()
        .checked_mul(size_of::<Pid>())
        .and_then(|size| c_int::try_from(size).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "PID buffer is too large"))?;
    // SAFETY: The mutable PID buffer is valid for exactly buffer_size bytes.
    let count = unsafe { proc_listallpids(process_ids.as_mut_ptr().cast(), buffer_size) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }
    process_ids.truncate((count as usize).min(process_ids.len()));
    process_ids.retain(|pid| *pid > 0);
    Ok(process_ids)
}

fn counters_for(pid: Pid) -> Option<Counters> {
    let mut info = MaybeUninit::<RUsageInfoV4>::zeroed();
    // SAFETY: info points to writable storage matching Darwin's rusage_info_v4 layout.
    let result = unsafe { proc_pid_rusage(pid, RUSAGE_INFO_V4, info.as_mut_ptr().cast()) };
    if result != 0 {
        return None;
    }
    // SAFETY: proc_pid_rusage returned success and initialized the structure.
    let info = unsafe { info.assume_init() };
    Some(Counters {
        cpu_nanoseconds: info.user_time.saturating_add(info.system_time),
        memory_bytes: info.phys_footprint,
        disk_read_bytes: info.diskio_bytesread,
        disk_write_bytes: info.diskio_byteswritten,
        process_start_abstime: info.proc_start_abstime,
    })
}

fn bsd_info_for(pid: Pid) -> Option<ProcBsdInfo> {
    let mut info = MaybeUninit::<ProcBsdInfo>::zeroed();
    let expected_size = c_int::try_from(size_of::<ProcBsdInfo>()).ok()?;
    // SAFETY: info points to writable storage matching Darwin's proc_bsdinfo layout.
    let result = unsafe {
        proc_pidinfo(
            pid,
            PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            expected_size,
        )
    };
    if result != expected_size {
        return None;
    }
    // SAFETY: proc_pidinfo returned the exact structure size.
    Some(unsafe { info.assume_init() })
}

fn executable_path_for(pid: Pid) -> Option<String> {
    let mut buffer = [0_u8; MAX_PATH_BUFFER];
    // SAFETY: buffer is valid writable memory for buffer.len() bytes.
    let length = unsafe {
        proc_pidpath(
            pid,
            buffer.as_mut_ptr().cast(),
            c_uint::try_from(buffer.len()).ok()?,
        )
    };
    decode_buffer(&buffer, length)
}

fn process_name_for(pid: Pid) -> Option<String> {
    let mut buffer = [0_u8; 2 * MAXCOMLEN + 1];
    // SAFETY: buffer is valid writable memory for buffer.len() bytes.
    let length = unsafe {
        proc_name(
            pid,
            buffer.as_mut_ptr().cast(),
            c_uint::try_from(buffer.len()).ok()?,
        )
    };
    decode_buffer(&buffer, length)
}

fn decode_buffer(buffer: &[u8], length: c_int) -> Option<String> {
    if length <= 0 {
        return None;
    }
    let length = (length as usize).min(buffer.len());
    let bytes = &buffer[..length];
    let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
    let value = String::from_utf8_lossy(bytes).into_owned();
    (!value.is_empty()).then_some(value)
}

fn unix_timestamp_millis() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_record(pid: Pid, parent_pid: Pid, executable_path: Option<&str>) -> ProcessRecord {
        ProcessRecord {
            pid,
            parent_pid,
            uid: 501,
            name: format!("pid-{pid}"),
            executable_path: executable_path.map(str::to_owned),
            counters: Counters {
                cpu_nanoseconds: 0,
                memory_bytes: 0,
                disk_read_bytes: 0,
                disk_write_bytes: 0,
                process_start_abstime: 1,
            },
        }
    }

    #[test]
    fn extracts_outermost_application_bundle() {
        let helper = "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper";
        assert_eq!(
            app_root_path(helper).as_deref(),
            Some("/Applications/Google Chrome.app")
        );
        assert_eq!(app_root_path("/usr/libexec/WindowServer"), None);
    }

    #[test]
    fn ffi_structures_match_darwin_layouts() {
        assert_eq!(size_of::<ProcBsdInfo>(), 136);
        assert_eq!(size_of::<RUsageInfoV4>(), 296);
    }

    #[test]
    fn inherits_application_bundle_from_parent_process() {
        let parent = test_record(
            10,
            1,
            Some("/Applications/Terminal.app/Contents/MacOS/Terminal"),
        );
        let child = test_record(11, 10, Some("/usr/bin/login"));
        let records = HashMap::from([(parent.pid, &parent), (child.pid, &child)]);

        assert_eq!(
            resolved_app_root(&child, &records).as_deref(),
            Some("/Applications/Terminal.app")
        );
    }

    #[test]
    fn parent_cycles_do_not_loop_forever() {
        let first = test_record(10, 11, None);
        let second = test_record(11, 10, None);
        let records = HashMap::from([(first.pid, &first), (second.pid, &second)]);

        assert_eq!(resolved_app_root(&first, &records), None);
    }

    #[test]
    fn counter_reset_does_not_underflow() {
        assert_eq!(positive_delta(4, 9), 0);
        assert_eq!(positive_delta(12, 9), 3);
    }

    #[test]
    fn one_logical_core_is_one_hundred_percent() {
        assert_eq!(cpu_percent(500_000_000, 1.0), 50.0);
        assert_eq!(cpu_percent(2_000_000_000, 1.0), 200.0);
        assert_eq!(cpu_percent(1, 0.0), 0.0);
    }

    #[test]
    fn samples_running_processes() {
        let snapshot = ProcessCollector::new().sample().expect("process sample");

        assert!(!snapshot.applications.is_empty());
        assert!(snapshot.total_memory_bytes() > 0);
        assert!(
            snapshot
                .applications
                .iter()
                .all(|application| !application.processes.is_empty())
        );
    }
}
