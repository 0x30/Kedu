use std::{env, path::PathBuf};

use anyhow::{Context, Result};

pub const SERVICE_LABEL: &str = "io.github.0x30.kedu";

pub fn config_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("KEDU_CONFIG_PATH").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = dirs::home_dir().context("无法确定用户主目录")?;
    Ok(home.join(".config/kedu/config.toml"))
}

pub fn state_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os("KEDU_STATE_DIR").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let root = dirs::data_dir().context("无法确定用户数据目录")?;
    Ok(root.join("Kedu"))
}

pub fn socket_path() -> Result<PathBuf> {
    Ok(state_dir()?.join("kedu.sock"))
}

pub fn history_database_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("KEDU_HISTORY_PATH").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    Ok(state_dir()?.join("history.sqlite3"))
}

pub fn log_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("无法确定用户主目录")?;
    Ok(home.join("Library/Logs/Kedu"))
}

pub fn launch_agent_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("无法确定用户主目录")?;
    Ok(home
        .join("Library/LaunchAgents")
        .join(format!("{SERVICE_LABEL}.plist")))
}
