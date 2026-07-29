use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use rusqlite::{Connection, Transaction, TransactionBehavior, params};

use crate::collector::ProcessSnapshot;

const DATABASE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS snapshots (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_unix_ms INTEGER NOT NULL,
    snapshot_json     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS snapshots_timestamp
    ON snapshots (timestamp_unix_ms, id);

PRAGMA user_version = 1;
"#;

pub struct HistoryDatabase {
    connection: Connection,
}

impl HistoryDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("无法创建历史数据库目录 {}", parent.display()))?;
        }

        let connection = Connection::open(path)
            .with_context(|| format!("无法打开历史数据库 {}", path.display()))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA foreign_keys = ON;",
            )
            .context("无法配置历史数据库")?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .context("无法配置历史数据库忙等待时间")?;
        connection
            .execute_batch(DATABASE_SCHEMA)
            .context("无法初始化历史数据库")?;

        Ok(Self { connection })
    }

    pub fn load(
        &self,
        retention: Duration,
        maximum_samples: usize,
    ) -> Result<Vec<ProcessSnapshot>> {
        if maximum_samples == 0 {
            return Ok(Vec::new());
        }

        let cutoff = unix_timestamp_millis().saturating_sub(duration_millis(retention));
        let maximum_samples = i64::try_from(maximum_samples).unwrap_or(i64::MAX);
        let mut statement = self
            .connection
            .prepare(
                "SELECT snapshot_json
                 FROM snapshots
                 WHERE timestamp_unix_ms >= ?1
                 ORDER BY timestamp_unix_ms DESC, id DESC
                 LIMIT ?2",
            )
            .context("无法准备历史数据查询")?;
        let rows = statement
            .query_map(params![cutoff, maximum_samples], |row| {
                row.get::<_, String>(0)
            })
            .context("无法查询历史数据")?;

        let mut snapshots = Vec::new();
        for row in rows {
            let json = row.context("无法读取历史数据行")?;
            let mut snapshot: ProcessSnapshot =
                serde_json::from_str(&json).context("无法解析历史快照")?;
            compact(&mut snapshot);
            snapshots.push(snapshot);
        }
        snapshots.reverse();
        Ok(snapshots)
    }

    pub fn append(
        &mut self,
        snapshot: &ProcessSnapshot,
        retention: Duration,
        maximum_samples: usize,
    ) -> Result<()> {
        let mut compacted = snapshot.clone();
        compact(&mut compacted);
        let json = serde_json::to_string(&compacted).context("无法序列化历史快照")?;

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("无法开始历史数据库事务")?;
        transaction
            .execute(
                "INSERT INTO snapshots (timestamp_unix_ms, snapshot_json) VALUES (?1, ?2)",
                params![compacted.timestamp_unix_ms, json],
            )
            .context("无法写入历史快照")?;
        trim(&transaction, retention, maximum_samples)?;
        transaction.commit().context("无法提交历史快照")
    }

    pub fn clear(&mut self) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("无法开始清空历史数据库事务")?;
        transaction
            .execute("DELETE FROM snapshots", [])
            .context("无法清空历史数据")?;
        transaction.commit().context("无法提交历史清空操作")?;
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .context("无法截断历史数据库 WAL")?;
        Ok(())
    }
}

fn trim(transaction: &Transaction<'_>, retention: Duration, maximum_samples: usize) -> Result<()> {
    let newest_timestamp_unix_ms: i64 = transaction
        .query_row("SELECT MAX(timestamp_unix_ms) FROM snapshots", [], |row| {
            row.get::<_, Option<i64>>(0)
        })
        .context("无法读取最新历史时间")?
        .unwrap_or_default();
    let cutoff = newest_timestamp_unix_ms.saturating_sub(duration_millis(retention));
    transaction
        .execute(
            "DELETE FROM snapshots WHERE timestamp_unix_ms < ?1",
            [cutoff],
        )
        .context("无法按保留时间裁剪历史数据")?;

    let count: i64 = transaction
        .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))
        .context("无法统计历史数据")?;
    let maximum_samples = i64::try_from(maximum_samples).unwrap_or(i64::MAX);
    let excess = count.saturating_sub(maximum_samples);
    if excess > 0 {
        transaction
            .execute(
                "DELETE FROM snapshots
                 WHERE id IN (
                     SELECT id
                     FROM snapshots
                     ORDER BY timestamp_unix_ms ASC, id ASC
                     LIMIT ?1
                 )",
                [excess],
            )
            .context("无法按数量裁剪历史数据")?;
    }
    Ok(())
}

fn compact(snapshot: &mut ProcessSnapshot) {
    for application in &mut snapshot.applications {
        application.processes.clear();
    }
}

fn duration_millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn unix_timestamp_millis() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        process,
        sync::atomic::{AtomicU64, Ordering},
    };

    use crate::collector::{ApplicationIdentity, ApplicationSample, ProcessSample};

    use super::*;

    static NEXT_DATABASE_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDatabasePath(PathBuf);

    impl TestDatabasePath {
        fn new() -> Self {
            let id = NEXT_DATABASE_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "kedu-history-test-{}-{}-{id}.sqlite3",
                process::id(),
                unix_timestamp_millis()
            ));
            Self(path)
        }
    }

    impl Drop for TestDatabasePath {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
            let _ = fs::remove_file(self.0.with_extension("sqlite3-shm"));
            let _ = fs::remove_file(self.0.with_extension("sqlite3-wal"));
        }
    }

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
    fn restores_compacted_history_after_reopening() {
        let path = TestDatabasePath::new();
        let now = unix_timestamp_millis();
        {
            let mut database = HistoryDatabase::open(&path.0).expect("open database");
            database
                .append(&snapshot(now), Duration::from_secs(60), 10)
                .expect("append snapshot");
        }

        let database = HistoryDatabase::open(&path.0).expect("reopen database");
        let restored = database
            .load(Duration::from_secs(60), 10)
            .expect("load history");

        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].timestamp_unix_ms, now);
        assert!(restored[0].applications[0].processes.is_empty());
    }

    #[test]
    fn trims_history_by_retention_and_count() {
        let path = TestDatabasePath::new();
        let now = unix_timestamp_millis();
        let mut database = HistoryDatabase::open(&path.0).expect("open database");
        for offset in [0, 1_000, 2_000, 3_000] {
            database
                .append(
                    &snapshot(now.saturating_sub(3_000).saturating_add(offset)),
                    Duration::from_secs(2),
                    2,
                )
                .expect("append snapshot");
        }
        database
            .append(
                &snapshot(now.saturating_sub(10_000)),
                Duration::from_secs(2),
                10,
            )
            .expect("append out-of-order snapshot");

        let restored = database
            .load(Duration::from_secs(60), 10)
            .expect("load history");
        let timestamps: Vec<_> = restored
            .iter()
            .map(|snapshot| snapshot.timestamp_unix_ms)
            .collect();

        assert_eq!(timestamps, vec![now - 1_000, now]);
    }

    #[test]
    fn clears_persisted_history() {
        let path = TestDatabasePath::new();
        let now = unix_timestamp_millis();
        let mut database = HistoryDatabase::open(&path.0).expect("open database");
        database
            .append(&snapshot(now), Duration::from_secs(60), 10)
            .expect("append snapshot");
        database.clear().expect("clear history");
        drop(database);

        let database = HistoryDatabase::open(&path.0).expect("reopen database");
        assert!(
            database
                .load(Duration::from_secs(60), 10)
                .expect("load history")
                .is_empty()
        );
    }
}
