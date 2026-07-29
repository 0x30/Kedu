use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixStream, unix::OwnedWriteHalf},
};

use crate::collector::ProcessSnapshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientRequest {
    Subscribe,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    State {
        history: Vec<ProcessSnapshot>,
        latest: Option<ProcessSnapshot>,
    },
    Snapshot {
        snapshot: ProcessSnapshot,
    },
    Pong,
    Error {
        message: String,
    },
}

pub async fn subscribe(path: &Path) -> Result<tokio::sync::mpsc::Receiver<ServerMessage>> {
    let stream = UnixStream::connect(path)
        .await
        .with_context(|| format!("无法连接监控服务 {}，请先运行 kedu start", path.display()))?;
    let (reader, mut writer) = stream.into_split();
    write_json_line(&mut writer, &ClientRequest::Subscribe).await?;

    let (sender, receiver) = tokio::sync::mpsc::channel(32);
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            match serde_json::from_str::<ServerMessage>(&line) {
                Ok(message) => {
                    if sender.send(message).await.is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = sender
                        .send(ServerMessage::Error {
                            message: format!("服务返回了无效数据：{error}"),
                        })
                        .await;
                    break;
                }
            }
        }
    });
    Ok(receiver)
}

pub async fn write_json_line<T: Serialize>(writer: &mut OwnedWriteHalf, value: &T) -> Result<()> {
    let mut data = serde_json::to_vec(value).context("无法序列化 IPC 消息")?;
    data.push(b'\n');
    writer.write_all(&data).await.context("无法写入 IPC 消息")
}
