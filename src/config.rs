use std::{fs, path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub sampling: SamplingConfig,
    pub metrics: MetricsConfig,
    pub display: DisplayConfig,
    pub daemon: DaemonConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SamplingConfig {
    #[serde(with = "humantime_serde")]
    pub interval: Duration,
    #[serde(with = "humantime_serde")]
    pub retention: Duration,
    pub maximum_samples: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsConfig {
    pub cpu: bool,
    pub memory: bool,
    pub disk: bool,
    pub network: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    pub top_applications: usize,
    pub maximum_chart_points: usize,
    pub mouse: bool,
    pub color: ColorMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub log_level: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    #[default]
    Auto,
    Truecolor,
    Ansi256,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            sampling: SamplingConfig::default(),
            metrics: MetricsConfig::default(),
            display: DisplayConfig::default(),
            daemon: DaemonConfig::default(),
        }
    }
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            retention: Duration::from_secs(30 * 60),
            maximum_samples: 21_600,
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            cpu: true,
            memory: true,
            disk: true,
            network: true,
        }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            top_applications: 7,
            maximum_chart_points: 600,
            mouse: true,
            color: ColorMode::Auto,
        }
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            log_level: "warn".to_owned(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = paths::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("无法读取配置文件 {}", path.display()))?;
        let config: Self = toml::from_str(&text)
            .with_context(|| format!("无法解析配置文件 {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn init(force: bool) -> Result<std::path::PathBuf> {
        let path = paths::config_path()?;
        if path.exists() && !force {
            bail!("配置文件已存在：{}", path.display());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("无法创建配置目录 {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(&Self::default()).context("无法生成默认配置")?;
        fs::write(&path, text).with_context(|| format!("无法写入配置文件 {}", path.display()))?;
        Ok(path)
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("不支持的配置版本：{}", self.version);
        }
        if self.sampling.interval < Duration::from_secs(1) {
            bail!("sampling.interval 不能小于 1 秒");
        }
        if self.sampling.retention < self.sampling.interval {
            bail!("sampling.retention 必须大于采样间隔");
        }
        if self.sampling.maximum_samples < 2 {
            bail!("sampling.maximum_samples 不能小于 2");
        }
        if self.display.top_applications == 0 {
            bail!("display.top_applications 不能为 0");
        }
        if self.display.maximum_chart_points < 2 {
            bail!("display.maximum_chart_points 不能小于 2");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_round_trips() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        let decoded: Config = toml::from_str(&text).unwrap();
        decoded.validate().unwrap();
        assert_eq!(decoded.sampling.interval, Duration::from_secs(5));
    }

    #[test]
    fn rejects_subsecond_sampling() {
        let mut config = Config::default();
        config.sampling.interval = Duration::from_millis(500);
        assert!(config.validate().is_err());
    }
}
