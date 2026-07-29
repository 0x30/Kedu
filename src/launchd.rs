use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};

pub const LABEL: &str = "io.github.0x30.kedu";
const UNLOAD_TIMEOUT: Duration = Duration::from_secs(5);
const UNLOAD_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceStatus {
    Stopped,
    Loaded,
    Running { pid: Option<u32> },
}

impl ServiceStatus {
    pub fn is_running(self) -> bool {
        matches!(self, Self::Running { .. })
    }

    pub fn is_loaded(self) -> bool {
        !matches!(self, Self::Stopped)
    }
}

impl fmt::Display for ServiceStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped => formatter.write_str("stopped"),
            Self::Loaded => formatter.write_str("loaded"),
            Self::Running { pid: Some(pid) } => write!(formatter, "running (pid {pid})"),
            Self::Running { pid: None } => formatter.write_str("running"),
        }
    }
}

pub fn start() -> Result<()> {
    let paths = ServicePaths::discover()?;
    let uid = current_uid()?;

    if status_for_uid(uid)?.is_loaded() {
        return Ok(());
    }

    fs::create_dir_all(&paths.launch_agents_dir).with_context(|| {
        format!(
            "failed to create launch agents directory {}",
            paths.launch_agents_dir.display()
        )
    })?;
    fs::create_dir_all(&paths.log_dir)
        .with_context(|| format!("failed to create log directory {}", paths.log_dir.display()))?;

    let plist = render_plist(&paths.executable, &paths.stdout_log, &paths.stderr_log);
    write_plist(&paths.plist, &plist)?;

    let domain = user_domain(uid);
    let output = launchctl([
        OsStr::new("bootstrap"),
        domain.as_os_str(),
        paths.plist.as_os_str(),
    ])?;
    if !output.status.success() {
        let _ = fs::remove_file(&paths.plist);
        return Err(command_error("launchctl bootstrap", &output));
    }

    Ok(())
}

pub fn stop() -> Result<()> {
    let paths = ServicePaths::discover()?;
    let uid = current_uid()?;

    if status_for_uid(uid)?.is_loaded() {
        let target = service_target(uid);
        let output = launchctl([OsStr::new("bootout"), target.as_os_str()])?;
        if !output.status.success() {
            return Err(command_error("launchctl bootout", &output));
        }
        wait_until_stopped(uid)?;
    }

    match fs::remove_file(&paths.plist) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("failed to remove launch agent {}", paths.plist.display())),
    }
}

pub fn restart() -> Result<()> {
    stop()?;
    start()
}

pub fn status() -> Result<ServiceStatus> {
    status_for_uid(current_uid()?)
}

pub fn plist_path() -> Result<PathBuf> {
    Ok(ServicePaths::discover()?.plist)
}

fn status_for_uid(uid: u32) -> Result<ServiceStatus> {
    let target = service_target(uid);
    let output = launchctl([OsStr::new("print"), target.as_os_str()])?;
    if !output.status.success() {
        if service_is_missing(&output) {
            return Ok(ServiceStatus::Stopped);
        }
        return Err(command_error("launchctl print", &output));
    }

    let description = String::from_utf8_lossy(&output.stdout);
    let state = launchctl_value(&description, "state");
    let pid = launchctl_value(&description, "pid").and_then(|value| value.parse().ok());

    if state == Some("running") {
        Ok(ServiceStatus::Running { pid })
    } else {
        Ok(ServiceStatus::Loaded)
    }
}

fn wait_until_stopped(uid: u32) -> Result<()> {
    let deadline = Instant::now() + UNLOAD_TIMEOUT;
    loop {
        if status_for_uid(uid)? == ServiceStatus::Stopped {
            return Ok(());
        }
        if Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for launchd service {LABEL} to stop");
        }
        thread::sleep(UNLOAD_POLL_INTERVAL);
    }
}

#[derive(Debug)]
struct ServicePaths {
    executable: PathBuf,
    launch_agents_dir: PathBuf,
    plist: PathBuf,
    log_dir: PathBuf,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
}

impl ServicePaths {
    fn discover() -> Result<Self> {
        let home = env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                anyhow!("HOME is not set; cannot locate the user LaunchAgents directory")
            })?;
        let launch_agents_dir = home.join("Library/LaunchAgents");
        let log_dir = home.join("Library/Logs/Kedu");

        Ok(Self {
            executable: stable_executable_path()?,
            plist: launch_agents_dir.join(format!("{LABEL}.plist")),
            stdout_log: log_dir.join("kedu.log"),
            stderr_log: log_dir.join("kedu-error.log"),
            launch_agents_dir,
            log_dir,
        })
    }
}

fn stable_executable_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("KEDU_EXECUTABLE_PATH").filter(|value| !value.is_empty()) {
        return absolute_path(PathBuf::from(path));
    }

    let current_executable = env::current_exe().context("failed to locate the kedu executable")?;
    if let Some(path) = executable_on_path("kedu", &current_executable) {
        // Keep the Homebrew bin symlink instead of resolving it into a versioned Cellar path.
        return Ok(path);
    }

    if let Some(argument_zero) = env::args_os().next() {
        let argument_path = PathBuf::from(&argument_zero);
        if argument_path.components().count() > 1 {
            return absolute_path(argument_path);
        }
    }

    Ok(current_executable)
}

fn absolute_path(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path);
    }

    Ok(env::current_dir()
        .context("failed to read the current directory")?
        .join(path))
}

fn executable_on_path(name: &str, current_executable: &Path) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    let current_executable = fs::canonicalize(current_executable).ok()?;
    env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| {
            fs::metadata(candidate)
                .map(|metadata| {
                    metadata.is_file()
                        && metadata.permissions().mode() & 0o111 != 0
                        && fs::canonicalize(candidate).ok().as_ref() == Some(&current_executable)
                })
                .unwrap_or(false)
        })
}

fn current_uid() -> Result<u32> {
    let output = Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .context("failed to execute /usr/bin/id -u")?;
    if !output.status.success() {
        return Err(command_error("/usr/bin/id -u", &output));
    }

    let uid = String::from_utf8_lossy(&output.stdout);
    uid.trim()
        .parse()
        .with_context(|| format!("invalid uid returned by /usr/bin/id: {uid:?}"))
}

fn user_domain(uid: u32) -> OsString {
    OsString::from(format!("gui/{uid}"))
}

fn service_target(uid: u32) -> OsString {
    OsString::from(format!("gui/{uid}/{LABEL}"))
}

fn launchctl<const N: usize>(arguments: [&OsStr; N]) -> Result<Output> {
    Command::new("/bin/launchctl")
        .args(arguments)
        .output()
        .context("failed to execute /bin/launchctl")
}

fn command_error(command: &str, output: &Output) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if !stderr.trim().is_empty() {
        stderr.trim()
    } else if !stdout.trim().is_empty() {
        stdout.trim()
    } else {
        "command returned no diagnostic output"
    };
    anyhow!("{command} failed with {}: {detail}", output.status)
}

fn service_is_missing(output: &Output) -> bool {
    String::from_utf8_lossy(&output.stderr).contains("Could not find service")
        || String::from_utf8_lossy(&output.stdout).contains("Could not find service")
}

fn launchctl_value<'a>(description: &'a str, key: &str) -> Option<&'a str> {
    description.lines().find_map(|line| {
        let (candidate, value) = line.trim().split_once('=')?;
        (candidate.trim() == key).then(|| value.trim())
    })
}

fn write_plist(path: &Path, contents: &str) -> Result<()> {
    let temporary = path.with_extension("plist.tmp");
    fs::write(&temporary, contents)
        .with_context(|| format!("failed to write launch agent {}", temporary.display()))?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o644)).with_context(|| {
        format!(
            "failed to set permissions on launch agent {}",
            temporary.display()
        )
    })?;
    fs::rename(&temporary, path).with_context(|| {
        format!(
            "failed to install launch agent {} from {}",
            path.display(),
            temporary.display()
        )
    })
}

fn render_plist(executable: &Path, stdout_log: &Path, stderr_log: &Path) -> String {
    let executable = xml_escape(&executable.to_string_lossy());
    let stdout_log = xml_escape(&stdout_log.to_string_lossy());
    let stderr_log = xml_escape(&stderr_log.to_string_lossy());

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{executable}</string>
        <string>daemon</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ProcessType</key>
    <string>Background</string>
    <key>StandardOutPath</key>
    <string>{stdout_log}</string>
    <key>StandardErrorPath</key>
    <string>{stderr_log}</string>
</dict>
</plist>
"#
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_contains_daemon_arguments_and_launch_policy() {
        let plist = render_plist(
            Path::new("/opt/homebrew/bin/kedu"),
            Path::new("/Users/test/Library/Logs/Kedu/kedu.log"),
            Path::new("/Users/test/Library/Logs/Kedu/kedu-error.log"),
        );

        assert!(plist.contains("<string>io.github.0x30.kedu</string>"));
        assert!(plist.contains("<string>/opt/homebrew/bin/kedu</string>"));
        assert!(plist.contains("<string>daemon</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n    <true/>"));
        assert!(plist.contains("<key>KeepAlive</key>\n    <true/>"));
        assert!(plist.contains("<string>/Users/test/Library/Logs/Kedu/kedu.log</string>"));
        assert!(plist.contains("<string>/Users/test/Library/Logs/Kedu/kedu-error.log</string>"));
    }

    #[test]
    fn plist_escapes_paths_as_xml_text() {
        let plist = render_plist(
            Path::new("/Applications/Kedu & Sons/<kedu>"),
            Path::new("/tmp/a&b.log"),
            Path::new("/tmp/error.log"),
        );

        assert!(plist.contains("/Applications/Kedu &amp; Sons/&lt;kedu&gt;"));
        assert!(plist.contains("/tmp/a&amp;b.log"));
        assert!(!plist.contains("Kedu & Sons"));
    }

    #[test]
    fn parses_launchctl_print_values() {
        let description = "\n\tstate = running\n\tpid = 1234\n";

        assert_eq!(launchctl_value(description, "state"), Some("running"));
        assert_eq!(launchctl_value(description, "pid"), Some("1234"));
        assert_eq!(launchctl_value(description, "missing"), None);
    }
}
