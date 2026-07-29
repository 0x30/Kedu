use std::{path::PathBuf, process::ExitCode};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use kedu::{config::Config, daemon, launchd, paths, tui};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "kedu", version, about = "macOS 应用级终端资源监控器")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 启动后台监控服务
    Start,
    /// 停止后台监控服务
    Stop,
    /// 重启后台监控服务
    Restart,
    /// 查看后台服务状态
    Status,
    /// 管理配置文件
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// 在前台运行采集服务（由 launchd 使用）
    #[command(hide = true)]
    Daemon,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// 创建默认配置文件
    Init {
        /// 覆盖已有配置
        #[arg(long)]
        force: bool,
    },
    /// 检查配置文件
    Check,
    /// 显示配置文件路径
    Path,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("kedu: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => tui::run(Config::load()?).await,
        Some(Command::Start) => {
            ensure_config_exists()?;
            Config::load()?.validate()?;
            launchd::start()?;
            println!("刻度监控服务已启动");
            println!("运行 kedu 打开监控界面");
            Ok(())
        }
        Some(Command::Stop) => {
            launchd::stop()?;
            println!("刻度监控服务已停止");
            Ok(())
        }
        Some(Command::Restart) => {
            ensure_config_exists()?;
            Config::load()?.validate()?;
            launchd::restart()?;
            println!("刻度监控服务已重启");
            Ok(())
        }
        Some(Command::Status) => {
            let status = launchd::status()?;
            match status {
                launchd::ServiceStatus::Stopped => println!("服务状态：已停止"),
                launchd::ServiceStatus::Loaded => println!("服务状态：已加载，等待运行"),
                launchd::ServiceStatus::Running { pid: Some(pid) } => {
                    println!("服务状态：运行中（PID {pid}）");
                }
                launchd::ServiceStatus::Running { pid: None } => {
                    println!("服务状态：运行中");
                }
            }
            Ok(())
        }
        Some(Command::Config { command }) => run_config_command(command),
        Some(Command::Daemon) => {
            let config = Config::load()?;
            init_logging(&config.daemon.log_level)?;
            daemon::run(config).await
        }
    }
}

fn run_config_command(command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Init { force } => {
            let path = Config::init(force)?;
            println!("已创建配置文件：{}", path.display());
        }
        ConfigCommand::Check => {
            let path = paths::config_path()?;
            Config::load_from(&path)?;
            println!("配置有效：{}", path.display());
        }
        ConfigCommand::Path => println!("{}", paths::config_path()?.display()),
    }
    Ok(())
}

fn ensure_config_exists() -> Result<PathBuf> {
    let path = paths::config_path()?;
    if path.exists() {
        return Ok(path);
    }
    Config::init(false)
}

fn init_logging(level: &str) -> Result<()> {
    let filter =
        EnvFilter::try_new(level).with_context(|| format!("无效的 daemon.log_level：{level}"))?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .try_init()
        .map_err(|error| anyhow::anyhow!("无法初始化日志：{error}"))
}
