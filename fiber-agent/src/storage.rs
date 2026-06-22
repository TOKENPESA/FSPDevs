use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use crate::{agent_fnn_pubkey, MeshChannelState, MeshPulsePayload};

pub const DEFAULT_STATE_DIR: &str = ".fiber-agent";
pub const DEFAULT_RETENTION_HOURS: i64 = 48;

const SCHEMA_SQL: &str = r#"
-- 1. LOCAL CHANNEL SNAPSHOT CACHE
CREATE TABLE IF NOT EXISTS fnn_channels (
    channel_id TEXT PRIMARY KEY,
    peer_pubkey TEXT NOT NULL,
    local_balance_shannons INTEGER NOT NULL,
    remote_balance_shannons INTEGER NOT NULL,
    is_ready BOOLEAN NOT NULL DEFAULT 1,
    last_poll_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 2. OFFLINE TELEMETRY & ALERT QUEUE
CREATE TABLE IF NOT EXISTS offline_telemetry_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    retry_count INTEGER DEFAULT 0
);

-- 3. EDGE TRANSACTION & AUDIT LEDGER
CREATE TABLE IF NOT EXISTS edge_transaction_ledger (
    tx_hash TEXT PRIMARY KEY,
    direction TEXT CHECK(direction IN ('INBOUND', 'OUTBOUND', 'ROUTED')),
    amount_shannons INTEGER NOT NULL,
    fee_earned_shannons INTEGER DEFAULT 0,
    status TEXT CHECK(status IN ('PENDING', 'SETTLED', 'FAILED')),
    settled_at DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 4. PERFORMANCE INDEXES
CREATE INDEX IF NOT EXISTS idx_telemetry_queue_created ON offline_telemetry_queue(created_at);
CREATE INDEX IF NOT EXISTS idx_ledger_status ON edge_transaction_ledger(status);
"#;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalChannelCache {
    pub channel_id: String,
    pub peer_pubkey: String,
    pub local_balance_shannons: u64,
    pub remote_balance_shannons: u64,
    pub is_ready: bool,
    pub last_poll_timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueuedTelemetry {
    pub id: i64,
    pub event_type: String,
    pub payload: String,
    pub created_at: DateTime<Utc>,
    pub retry_count: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeTxRecord {
    pub tx_hash: String,
    pub direction: String,
    pub amount_shannons: u64,
    pub fee_earned_shannons: u64,
    pub status: String,
    pub settled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MaintenanceReport {
    pub evicted_telemetry: usize,
    pub failed_pending_tx: usize,
}

/// Opens SQLite with WAL + NORMAL synchronous for safe concurrent fleet writes.
pub fn initialize_local_storage_secure<P: AsRef<Path>>(db_path: P) -> SqliteResult<Connection> {
    let conn = Connection::open(db_path)?;

    // WAL enables concurrent readers/writers across mesh-fleet sidecar processes.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;

    println!("💾 [STORAGE] Secure SQLite backend active with WAL configuration.");
    Ok(conn)
}

/// SQLite-backed sidecar persistence (channels, telemetry queue, tx ledger).
#[derive(Debug)]
pub struct AgentDb {
    path: PathBuf,
    conn: Mutex<Connection>,
}

impl AgentDb {
    pub fn open(agent_id: u16) -> Result<Self, String> {
        Self::open_path(resolve_db_path(agent_id))
    }

    pub fn open_path(path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create dir {}: {err}", parent.display()))?;
        }

        let conn = initialize_local_storage_secure(&path)
            .map_err(|err| format!("open {}: {err}", path.display()))?;
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|err| format!("migrate {}: {err}", path.display()))?;

        let db = Self {
            path,
            conn: Mutex::new(conn),
        };
        db.run_retention_maintenance()?;
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn open_async(agent_id: u16) -> Result<Self, String> {
        let path = resolve_db_path(agent_id);
        tokio::task::spawn_blocking(move || Self::open_path(path))
            .await
            .map_err(|err| format!("open task join: {err}"))?
    }

    /// Replace the full FNN channel snapshot from a `list_channels` poll.
    pub fn replace_channel_snapshot(&self, channels: &[LocalChannelCache]) -> Result<(), String> {
        let mut conn = self.lock_conn()?;
        let tx = conn
            .transaction()
            .map_err(|err| format!("begin channel snapshot tx: {err}"))?;

        tx.execute("DELETE FROM fnn_channels", [])
            .map_err(|err| format!("clear fnn_channels: {err}"))?;

        for ch in channels {
            tx.execute(
                r#"
                INSERT INTO fnn_channels (
                    channel_id, peer_pubkey, local_balance_shannons,
                    remote_balance_shannons, is_ready, last_poll_timestamp
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    ch.channel_id,
                    ch.peer_pubkey,
                    ch.local_balance_shannons as i64,
                    ch.remote_balance_shannons as i64,
                    ch.is_ready,
                    format_sql_datetime(ch.last_poll_timestamp),
                ],
            )
            .map_err(|err| format!("insert fnn_channel {}: {err}", ch.channel_id))?;
        }

        tx.commit()
            .map_err(|err| format!("commit channel snapshot tx: {err}"))?;
        Ok(())
    }

    pub fn list_cached_channels(&self) -> Result<Vec<LocalChannelCache>, String> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT channel_id, peer_pubkey, local_balance_shannons,
                       remote_balance_shannons, is_ready, last_poll_timestamp
                FROM fnn_channels
                ORDER BY channel_id
                "#,
            )
            .map_err(|err| format!("prepare list fnn_channels: {err}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(LocalChannelCache {
                    channel_id: row.get(0)?,
                    peer_pubkey: row.get(1)?,
                    local_balance_shannons: row.get::<_, i64>(2)? as u64,
                    remote_balance_shannons: row.get::<_, i64>(3)? as u64,
                    is_ready: row.get(4)?,
                    last_poll_timestamp: read_sqlite_datetime(row, 5)?,
                })
            })
            .map_err(|err| format!("query fnn_channels: {err}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("collect fnn_channels: {err}"))
    }

    pub fn enqueue_telemetry(
        &self,
        event_type: &str,
        payload: &MeshPulsePayload,
    ) -> Result<i64, String> {
        let payload = serde_json::to_string(payload).map_err(|err| format!("encode telemetry: {err}"))?;
        self.enqueue_telemetry_raw(event_type, &payload)
    }

    pub fn enqueue_telemetry_raw(
        &self,
        event_type: &str,
        payload: &str,
    ) -> Result<i64, String> {
        let conn = self.lock_conn()?;
        conn.execute(
            r#"
            INSERT INTO offline_telemetry_queue (event_type, payload)
            VALUES (?1, ?2)
            "#,
            params![event_type, payload],
        )
        .map_err(|err| format!("enqueue telemetry: {err}"))?;
        Ok(conn.last_insert_rowid())
    }

    /// FIFO dequeue — oldest telemetry item first.
    pub fn dequeue_telemetry(&self) -> Result<Option<QueuedTelemetry>, String> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, event_type, payload, created_at, retry_count
                FROM offline_telemetry_queue
                ORDER BY created_at ASC, id ASC
                LIMIT 1
                "#,
            )
            .map_err(|err| format!("prepare dequeue telemetry: {err}"))?;

        let mut rows = stmt
            .query_map([], |row| {
                Ok(QueuedTelemetry {
                    id: row.get(0)?,
                    event_type: row.get(1)?,
                    payload: row.get(2)?,
                    created_at: read_sqlite_datetime(row, 3)?,
                    retry_count: row.get::<_, i32>(4)? as u32,
                })
            })
            .map_err(|err| format!("query dequeue telemetry: {err}"))?;

        let item = match rows.next() {
            Some(Ok(item)) => item,
            Some(Err(err)) => return Err(format!("read queued telemetry: {err}")),
            None => return Ok(None),
        };

        conn.execute(
            "DELETE FROM offline_telemetry_queue WHERE id = ?1",
            params![item.id],
        )
        .map_err(|err| format!("delete telemetry id {}: {err}", item.id))?;

        Ok(Some(item))
    }

    pub fn increment_telemetry_retry(&self, id: i64) -> Result<(), String> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE offline_telemetry_queue SET retry_count = retry_count + 1 WHERE id = ?1",
            params![id],
        )
        .map_err(|err| format!("increment telemetry retry {id}: {err}"))?;
        Ok(())
    }

    pub fn pending_telemetry_count(&self) -> Result<i64, String> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT COUNT(*) FROM offline_telemetry_queue",
            [],
            |row| row.get(0),
        )
        .map_err(|err| format!("count telemetry queue: {err}"))
    }

    pub fn insert_edge_transaction(&self, tx: &EdgeTxRecord) -> Result<(), String> {
        let conn = self.lock_conn()?;
        conn.execute(
            r#"
            INSERT INTO edge_transaction_ledger (
                tx_hash, direction, amount_shannons, fee_earned_shannons,
                status, settled_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                tx.tx_hash,
                tx.direction,
                tx.amount_shannons as i64,
                tx.fee_earned_shannons as i64,
                tx.status,
                tx.settled_at.map(format_sql_datetime),
            ],
        )
        .map_err(|err| format!("insert edge tx {}: {err}", tx.tx_hash))?;
        Ok(())
    }

    pub fn update_edge_transaction_status(
        &self,
        tx_hash: &str,
        status: &str,
        settled_at: Option<DateTime<Utc>>,
    ) -> Result<(), String> {
        let conn = self.lock_conn()?;
        conn.execute(
            r#"
            UPDATE edge_transaction_ledger
            SET status = ?1, settled_at = COALESCE(?2, settled_at)
            WHERE tx_hash = ?3
            "#,
            params![
                status,
                settled_at.map(format_sql_datetime),
                tx_hash,
            ],
        )
        .map_err(|err| format!("update edge tx {tx_hash}: {err}"))?;
        Ok(())
    }

    pub fn list_edge_transactions_by_status(
        &self,
        status: &str,
    ) -> Result<Vec<EdgeTxRecord>, String> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT tx_hash, direction, amount_shannons, fee_earned_shannons,
                       status, settled_at, created_at
                FROM edge_transaction_ledger
                WHERE status = ?1
                ORDER BY created_at ASC
                "#,
            )
            .map_err(|err| format!("prepare list edge tx: {err}"))?;

        let rows = stmt
            .query_map([status], |row| {
                let settled_at = match row.get::<_, Option<String>>(5)? {
                    Some(value) => Some(parse_datetime(&value).map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(err))
                    })?),
                    None => None,
                };

                Ok(EdgeTxRecord {
                    tx_hash: row.get(0)?,
                    direction: row.get(1)?,
                    amount_shannons: row.get::<_, i64>(2)? as u64,
                    fee_earned_shannons: row.get::<_, i64>(3)? as u64,
                    status: row.get(4)?,
                    settled_at,
                    created_at: read_sqlite_datetime(row, 6)?,
                })
            })
            .map_err(|err| format!("query edge tx: {err}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("collect edge tx: {err}"))
    }

    /// Evict stale telemetry and fail abandoned pending transactions (48-hour window).
    pub fn run_retention_maintenance(&self) -> Result<MaintenanceReport, String> {
        let hours = retention_hours();
        let conn = self.lock_conn()?;

        let evicted_telemetry = conn
            .execute(
                "DELETE FROM offline_telemetry_queue WHERE created_at < datetime('now', ?1)",
                params![format!("-{hours} hours")],
            )
            .map_err(|err| format!("evict stale telemetry: {err}"))?;

        let failed_pending_tx = conn
            .execute(
                r#"
                UPDATE edge_transaction_ledger
                SET status = 'FAILED'
                WHERE status = 'PENDING' AND created_at < datetime('now', ?1)
                "#,
                params![format!("-{hours} hours")],
            )
            .map_err(|err| format!("fail stale pending transactions: {err}"))?;

        Ok(MaintenanceReport {
            evicted_telemetry,
            failed_pending_tx,
        })
    }

    fn lock_conn(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.conn
            .lock()
            .map_err(|_| "database mutex poisoned".to_string())
    }
}

impl LocalChannelCache {
    pub fn from_mesh_channel(channel: &MeshChannelState) -> Self {
        let peer_pubkey = channel
            .peer_pubkey
            .clone()
            .unwrap_or_else(|| agent_fnn_pubkey(channel.peer_id));

        Self {
            channel_id: channel
                .channel_id
                .clone()
                .unwrap_or_else(|| peer_pubkey.clone()),
            peer_pubkey,
            local_balance_shannons: channel.local_balance_shannons,
            remote_balance_shannons: channel.remote_balance_shannons,
            is_ready: channel.is_active,
            last_poll_timestamp: Utc::now(),
        }
    }
}

pub fn channel_cache_from_mesh(channels: &[MeshChannelState]) -> Vec<LocalChannelCache> {
    channels
        .iter()
        .map(LocalChannelCache::from_mesh_channel)
        .collect()
}

pub fn resolve_db_path(agent_id: u16) -> PathBuf {
    if let Ok(path) = env::var("FIBER_AGENT_DB_PATH") {
        return PathBuf::from(path);
    }

    let dir = env::var("FIBER_AGENT_STATE_DIR").unwrap_or_else(|_| DEFAULT_STATE_DIR.to_string());
    PathBuf::from(dir).join(format!("fa-{agent_id}.db"))
}

fn retention_hours() -> i64 {
    env::var("FIBER_AGENT_RETENTION_HOURS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|hours| *hours > 0)
        .unwrap_or(DEFAULT_RETENTION_HOURS)
}

fn format_sql_datetime(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn read_sqlite_datetime(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<DateTime<Utc>> {
    let value: rusqlite::types::Value = row.get(idx)?;

    match value {
        rusqlite::types::Value::Text(text) => parse_datetime(&text).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(err))
        }),
        rusqlite::types::Value::Integer(secs) => DateTime::from_timestamp(secs, 0).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                idx,
                rusqlite::types::Type::Integer,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid unix timestamp: {secs}"),
                )),
            )
        }),
        rusqlite::types::Value::Real(secs) => DateTime::from_timestamp(secs as i64, 0).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                idx,
                rusqlite::types::Type::Real,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid unix timestamp: {secs}"),
                )),
            )
        }),
        rusqlite::types::Value::Null => Ok(Utc::now()),
        other => Err(rusqlite::Error::InvalidColumnType(
            idx,
            "datetime".into(),
            other.data_type(),
        )),
    }
}

#[derive(Debug)]
struct DatetimeParseError(String);

impl std::fmt::Display for DatetimeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for DatetimeParseError {}

fn parse_datetime(text: &str) -> Result<DateTime<Utc>, DatetimeParseError> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
        return Ok(dt.with_timezone(&Utc));
    }

    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S") {
        return Ok(naive.and_utc());
    }

    if let Ok(secs) = text.parse::<i64>() {
        return DateTime::from_timestamp(secs, 0)
            .ok_or_else(|| DatetimeParseError(format!("invalid unix timestamp: {secs}")));
    }

    Err(DatetimeParseError(format!("unparseable datetime: {text}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(agent_id: u16) -> (AgentDb, PathBuf) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "fiber-agent-db-test-{agent_id}-{unique}.db"
        ));
        let db = AgentDb::open_path(path.clone()).expect("open temp db");
        (db, path)
    }

    #[test]
    fn channel_snapshot_round_trip() {
        let (db, path) = temp_db(44);
        let channels = vec![LocalChannelCache {
            channel_id: "chan-45".to_string(),
            peer_pubkey: agent_fnn_pubkey(45),
            local_balance_shannons: 1_000_000,
            remote_balance_shannons: 2_000_000,
            is_ready: true,
            last_poll_timestamp: Utc::now(),
        }];

        db.replace_channel_snapshot(&channels)
            .expect("replace channel snapshot");
        let loaded = db.list_cached_channels().expect("list channels");
        assert_eq!(loaded.len(), channels.len());
        assert_eq!(loaded[0].channel_id, channels[0].channel_id);
        assert_eq!(loaded[0].peer_pubkey, channels[0].peer_pubkey);
        assert_eq!(loaded[0].local_balance_shannons, channels[0].local_balance_shannons);
        assert_eq!(loaded[0].is_ready, channels[0].is_ready);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn telemetry_queue_is_fifo() {
        let (db, path) = temp_db(44);
        let first = MeshPulsePayload {
            status: "MESH_HEARTBEAT".to_string(),
            agent: 44,
            active_mesh_neighbors: vec![45],
            report_target: 44,
            attempt: 0,
            timestamp: 1,
            public_key_hex: None,
            signature_hex: None,
            fnn_pubkey_hex: None,
            peer_connect_address: None,
            outbound_shannons: None,
            inbound_shannons: None,
        };
        let second = MeshPulsePayload {
            status: "ALERT_MFA_NODE_DROPPED".to_string(),
            agent: 44,
            active_mesh_neighbors: vec![],
            report_target: 45,
            attempt: 1,
            timestamp: 2,
            public_key_hex: None,
            signature_hex: None,
            fnn_pubkey_hex: None,
            peer_connect_address: None,
            outbound_shannons: None,
            inbound_shannons: None,
        };

        db.enqueue_telemetry("MESH_HEARTBEAT", &first)
            .expect("enqueue first");
        std::thread::sleep(std::time::Duration::from_millis(5));
        db.enqueue_telemetry("ALERT_MFA_NODE_DROPPED", &second)
            .expect("enqueue second");

        let dequeued = db.dequeue_telemetry().expect("dequeue").expect("item");
        assert_eq!(dequeued.event_type, "MESH_HEARTBEAT");
        let payload: MeshPulsePayload =
            serde_json::from_str(&dequeued.payload).expect("decode payload");
        assert_eq!(payload.status, "MESH_HEARTBEAT");

        let next = db.dequeue_telemetry().expect("dequeue next").expect("item");
        assert_eq!(next.event_type, "ALERT_MFA_NODE_DROPPED");
        assert!(db.dequeue_telemetry().expect("empty").is_none());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn edge_ledger_filters_by_status() {
        let (db, path) = temp_db(44);
        let pending = EdgeTxRecord {
            tx_hash: "tx-pending".to_string(),
            direction: "ROUTED".to_string(),
            amount_shannons: 500_000,
            fee_earned_shannons: 1_000,
            status: "PENDING".to_string(),
            settled_at: None,
            created_at: Utc::now(),
        };
        let settled = EdgeTxRecord {
            tx_hash: "tx-settled".to_string(),
            direction: "OUTBOUND".to_string(),
            amount_shannons: 900_000,
            fee_earned_shannons: 2_500,
            status: "SETTLED".to_string(),
            settled_at: Some(Utc::now()),
            created_at: Utc::now(),
        };

        db.insert_edge_transaction(&pending).expect("insert pending");
        db.insert_edge_transaction(&settled).expect("insert settled");

        let pending_rows = db
            .list_edge_transactions_by_status("PENDING")
            .expect("list pending");
        assert_eq!(pending_rows.len(), 1);
        assert_eq!(pending_rows[0].tx_hash, "tx-pending");

        db.update_edge_transaction_status("tx-pending", "FAILED", None)
            .expect("mark failed");
        assert!(db
            .list_edge_transactions_by_status("PENDING")
            .expect("list pending")
            .is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn retention_maintenance_evicts_stale_rows() {
        let (db, path) = temp_db(44);

        {
            let conn = db.lock_conn().expect("lock db");
            conn.execute(
                r#"
                INSERT INTO offline_telemetry_queue (event_type, payload, created_at)
                VALUES ('MESH_HEARTBEAT', '{}', datetime('now', '-49 hours'))
                "#,
                [],
            )
            .expect("insert stale telemetry");

            conn.execute(
                r#"
                INSERT INTO edge_transaction_ledger (
                    tx_hash, direction, amount_shannons, fee_earned_shannons,
                    status, created_at
                ) VALUES (
                    'tx-stale', 'ROUTED', 1000, 0, 'PENDING', datetime('now', '-49 hours')
                )
                "#,
                [],
            )
            .expect("insert stale pending tx");
        }

        let report = db.run_retention_maintenance().expect("run maintenance");
        assert_eq!(report.evicted_telemetry, 1);
        assert_eq!(report.failed_pending_tx, 1);
        assert_eq!(db.pending_telemetry_count().expect("count"), 0);

        let failed = db
            .list_edge_transactions_by_status("FAILED")
            .expect("list failed");
        assert!(failed.iter().any(|tx| tx.tx_hash == "tx-stale"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn secure_storage_uses_wal_journal_mode() {
        let path = std::env::temp_dir().join(format!(
            "fiber-agent-wal-test-{}.db",
            Utc::now().timestamp()
        ));
        let conn = initialize_local_storage_secure(&path).expect("open wal db");
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("read journal_mode");
        assert_eq!(mode.to_ascii_lowercase(), "wal");
        drop(conn);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn resolve_db_path_uses_agent_suffix() {
        let path = resolve_db_path(77);
        assert!(path.to_string_lossy().contains("fa-77.db"));
    }
}
