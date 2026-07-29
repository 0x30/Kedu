use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::{RwLock, broadcast},
    time::{MissedTickBehavior, interval},
};
use tracing::{error, info, warn};

use crate::{
    collector::{NetworkCollector, ProcessCollector, ProcessSnapshot},
    config::Config,
    history::HistoryStore,
    ipc::{ClientRequest, ServerMessage, write_json_line},
    paths,
    storage::HistoryDatabase,
};

pub async fn run(config: Config) -> Result<()> {
    config.validate()?;
    let socket_path = paths::socket_path()?;
    prepare_socket(&socket_path)?;
    let _cleanup = SocketCleanup(socket_path.clone());
    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("无法监听 IPC Socket {}", socket_path.display()))?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("无法设置 IPC Socket 权限 {}", socket_path.display()))?;

    let mut history_store =
        HistoryStore::new(config.sampling.retention, config.sampling.maximum_samples);
    let mut database = if config.storage.enabled {
        let path = paths::history_database_path()?;
        let database = HistoryDatabase::open(&path)?;
        for snapshot in database.load(config.sampling.retention, config.sampling.maximum_samples)? {
            history_store.append(snapshot);
        }
        info!(path = %path.display(), "restored persisted history");
        Some(database)
    } else {
        None
    };
    let history = Arc::new(RwLock::new(history_store));
    let (updates, _) = broadcast::channel(32);
    let mut process_collector = ProcessCollector::new();
    let mut network_collector = NetworkCollector::new();

    capture(
        &config,
        &mut process_collector,
        &mut network_collector,
        database.as_mut(),
        &history,
        &updates,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    capture(
        &config,
        &mut process_collector,
        &mut network_collector,
        database.as_mut(),
        &history,
        &updates,
    )
    .await;

    let mut ticker = interval(config.sampling.interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ticker.tick().await;
    info!(socket = %socket_path.display(), "kedu daemon started");

    loop {
        tokio::select! {
            connection = listener.accept() => {
                match connection {
                    Ok((stream, _)) => {
                        let history = Arc::clone(&history);
                        let updates = updates.clone();
                        tokio::spawn(async move {
                            if let Err(error) = serve_client(stream, history, updates).await {
                                warn!(%error, "IPC client disconnected with an error");
                            }
                        });
                    }
                    Err(error) => warn!(%error, "failed to accept IPC client"),
                }
            }
            _ = ticker.tick() => {
                capture(
                    &config,
                    &mut process_collector,
                    &mut network_collector,
                    database.as_mut(),
                    &history,
                    &updates,
                ).await;
            }
            _ = shutdown_signal() => {
                info!("kedu daemon stopping");
                break;
            }
        }
    }
    Ok(())
}

async fn capture(
    config: &Config,
    process_collector: &mut ProcessCollector,
    network_collector: &mut NetworkCollector,
    database: Option<&mut HistoryDatabase>,
    history: &Arc<RwLock<HistoryStore>>,
    updates: &broadcast::Sender<ProcessSnapshot>,
) {
    let mut snapshot = match process_collector.sample() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            error!(%error, "process collection failed");
            return;
        }
    };

    if config.metrics.network {
        let identities = process_collector.identities_by_pid();
        match network_collector.sample_rates(&identities) {
            Ok(rates) => snapshot.merge_network(rates),
            Err(error) => warn!(%error, "network collection failed"),
        }
    }
    apply_metric_selection(&mut snapshot, config);

    if let Some(database) = database
        && let Err(error) = database.append(
            &snapshot,
            config.sampling.retention,
            config.sampling.maximum_samples,
        )
    {
        error!(%error, "history persistence failed");
    }
    history.write().await.append(snapshot.clone());
    let _ = updates.send(snapshot);
}

fn apply_metric_selection(snapshot: &mut ProcessSnapshot, config: &Config) {
    for application in &mut snapshot.applications {
        if !config.metrics.cpu {
            application.cpu_percent = 0.0;
        }
        if !config.metrics.memory {
            application.memory_bytes = 0;
        }
        if !config.metrics.disk {
            application.disk_read_bytes_per_second = 0.0;
            application.disk_write_bytes_per_second = 0.0;
        }
        if !config.metrics.network {
            application.network_download_bytes_per_second = 0.0;
            application.network_upload_bytes_per_second = 0.0;
        }
        for process in &mut application.processes {
            if !config.metrics.cpu {
                process.cpu_percent = 0.0;
            }
            if !config.metrics.memory {
                process.memory_bytes = 0;
            }
            if !config.metrics.disk {
                process.disk_read_bytes_per_second = 0.0;
                process.disk_write_bytes_per_second = 0.0;
            }
            if !config.metrics.network {
                process.network_download_bytes_per_second = 0.0;
                process.network_upload_bytes_per_second = 0.0;
            }
        }
    }
}

async fn serve_client(
    stream: UnixStream,
    history: Arc<RwLock<HistoryStore>>,
    updates: broadcast::Sender<ProcessSnapshot>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let Some(line) = lines.next_line().await.context("无法读取 IPC 请求")? else {
        return Ok(());
    };
    let request: ClientRequest = serde_json::from_str(&line).context("无法解析 IPC 请求")?;
    match request {
        ClientRequest::Ping => write_json_line(&mut writer, &ServerMessage::Pong).await,
        ClientRequest::Subscribe => {
            let state = history.read().await;
            let history_snapshot = state.snapshots();
            let latest = state.latest();
            drop(state);
            write_json_line(
                &mut writer,
                &ServerMessage::State {
                    history: history_snapshot,
                    latest,
                },
            )
            .await?;

            let mut receiver = updates.subscribe();
            loop {
                match receiver.recv().await {
                    Ok(snapshot) => {
                        write_json_line(&mut writer, &ServerMessage::Snapshot { snapshot }).await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!(count, "IPC client lagged behind");
                        let state = history.read().await;
                        let history_snapshot = state.snapshots();
                        let latest = state.latest();
                        drop(state);
                        write_json_line(
                            &mut writer,
                            &ServerMessage::State {
                                history: history_snapshot,
                                latest,
                            },
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                }
            }
        }
    }
}

fn prepare_socket(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("无法创建状态目录 {}", parent.display()))?;
    }
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("无法移除旧 IPC Socket {}", path.display()));
        }
    }
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = terminate.recv() => {},
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

struct SocketCleanup(PathBuf);

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
