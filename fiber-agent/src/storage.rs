use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use mesh_core::error::MeshError;
use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::sanitize_storage_error;
use mesh_core::types::EdgeTransaction;
use crate::{agent_fnn_pubkey, MeshChannelState, MeshPulsePayload};

pub const DEFAULT_DB_WRITE_QUEUE_CAPACITY: usize = 256;

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

-- 5. MOBILE MONEY / FIAT BRIDGE EDGE LEDGER
CREATE TABLE IF NOT EXISTS fiat_edge_ledger (
    tx_id TEXT PRIMARY KEY,
    agent_id INTEGER NOT NULL,
    tx_type TEXT NOT NULL,
    asset TEXT NOT NULL,
    amount_atomic INTEGER NOT NULL,
    asset_capacities_json TEXT,
    fiat_amount REAL NOT NULL,
    counterparty_pubkey TEXT NOT NULL,
    payment_hash TEXT,
    preimage TEXT,
    timestamp INTEGER NOT NULL,
    is_synchronized INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_fiat_edge_agent ON fiat_edge_ledger(agent_id);

-- 6. DICOBA / JUNGUKUU MICRO-CONTRIBUTION RECEIPTS
CREATE TABLE IF NOT EXISTS dicoba_contributions (
    tx_id TEXT PRIMARY KEY,
    vault_id TEXT NOT NULL,
    group_name TEXT NOT NULL DEFAULT '',
    member_id TEXT NOT NULL DEFAULT '',
    amount_shannons INTEGER NOT NULL,
    timestamp INTEGER NOT NULL,
    synced INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_dicoba_contributions_vault ON dicoba_contributions(vault_id);

-- 7. OFFLINE UTILITY PAYMENT INTENTS (BatterySaver / disconnected edge)
CREATE TABLE IF NOT EXISTS utility_payment_intents (
    id INTEGER PRIMARY KEY,
    payment_hash TEXT NOT NULL,
    amount_shannons INTEGER NOT NULL,
    status TEXT NOT NULL,
    synced INTEGER DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_utility_payment_intents_status ON utility_payment_intents(status);

-- 8. TELCO B2C FLOAT ACCOUNTS (atomic sub-units)
CREATE TABLE IF NOT EXISTS telco_float_accounts (
    account_id TEXT PRIMARY KEY,
    provider TEXT NOT NULL DEFAULT '',
    live_balance_units INTEGER NOT NULL DEFAULT 0,
    critical_floor_units INTEGER NOT NULL DEFAULT 0,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 9. UI-MANAGED MODULE INSTALLATIONS (app store persistence)
CREATE TABLE IF NOT EXISTS installed_modules (
    id TEXT PRIMARY KEY,
    module_name TEXT NOT NULL UNIQUE,
    is_active INTEGER NOT NULL DEFAULT 1,
    config_json TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_installed_modules_active ON installed_modules(is_active);

-- 10. Module registry bootstrap marker (prevents profile re-seed after user uninstall)
CREATE TABLE IF NOT EXISTS sidecar_registry_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- 11. MFA-issued control-plane identity (FA-N + HMAC secret)
CREATE TABLE IF NOT EXISTS agent_identity (
    agent_id TEXT NOT NULL,
    agent_id_numeric INTEGER NOT NULL,
    agent_secret TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
"#;

/// Initializes the offline utility payment ledger (WAL + intent queue schema).
pub fn init_offline_ledger(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS utility_payment_intents (
            id INTEGER PRIMARY KEY,
            payment_hash TEXT NOT NULL,
            amount_shannons INTEGER NOT NULL,
            status TEXT NOT NULL,
            synced INTEGER DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_utility_payment_intents_status ON utility_payment_intents(status);
        ",
    )
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultContributorSummary {
    pub member_id: String,
    pub contribution_count: u64,
    pub total_shannons: u64,
    pub last_timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemberVaultSummary {
    pub group_name: String,
    pub vault_id: String,
    pub contribution_count: u64,
    pub total_shannons: u64,
    pub last_timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelcoFloatSnapshot {
    pub provider: String,
    pub account_id: String,
    pub live_balance_units: u64,
    pub critical_floor_units: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DashboardLedgerCounts {
    pub dicoba_contributions: u64,
    pub dicoba_vaults_total: u64,
    pub edge_pending: u64,
    pub edge_settled: u64,
    pub edge_failed: u64,
    pub fiat_edge_transactions: u64,
    pub queued_telemetry: u64,
    pub cached_channels: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MaintenanceReport {
    pub evicted_telemetry: usize,
    pub failed_pending_tx: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtilityPaymentIntent {
    pub id: i64,
    pub payment_hash: String,
    pub amount_shannons: u64,
    pub status: String,
    pub synced: bool,
}

/// Opens SQLite with WAL, NORMAL synchronous, and a 5s busy timeout for fleet sidecars.
pub fn open_performance_tuned_db<P: AsRef<Path>>(path: P) -> SqliteResult<Connection> {
    let conn = Connection::open(path)?;

    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;

    println!("💾 [SQLITE] High-velocity WAL mode engine mapping online.");
    Ok(conn)
}

/// SQLite-backed sidecar persistence (channels, telemetry queue, tx ledger).
#[derive(Debug)]
pub struct AgentDb {
    path: PathBuf,
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledModuleRecord {
    pub id: String,
    pub module_name: String,
    pub is_active: bool,
    pub config_json: String,
}

impl AgentDb {
    pub fn open(agent_id: u16) -> Result<Self, String> {
        Self::open_path(resolve_db_path(agent_id))
    }

    pub fn open_path(path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| sanitize_storage_error("create database directory", err))?;
        }

        let conn = open_performance_tuned_db(&path)
            .map_err(|err| sanitize_storage_error("open database", err))?;
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|err| sanitize_storage_error("migrate schema", err))?;
        migrate_dicoba_contributions_columns(&conn)
            .map_err(|err| sanitize_storage_error("migrate dicoba columns", err))?;
        migrate_fiat_edge_capacities_column(&conn)
            .map_err(|err| sanitize_storage_error("migrate fiat edge capacities", err))?;
        migrate_module_registry_meta(&conn)
            .map_err(|err| sanitize_storage_error("migrate module registry meta", err))?;
        migrate_agent_identity_table(&conn)
            .map_err(|err| sanitize_storage_error("migrate agent identity", err))?;
        init_offline_ledger(&conn)
            .map_err(|err| sanitize_storage_error("init offline ledger", err))?;

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
            .map_err(|err| sanitize_storage_error("open task join", err))?
    }

    /// Replace the full FNN channel snapshot from a `list_channels` poll.
    pub fn replace_channel_snapshot(&self, channels: &[LocalChannelCache]) -> Result<(), String> {
        let mut conn = self.lock_conn()?;
        let tx = conn
            .transaction()
            .map_err(|err| sanitize_storage_error("begin channel snapshot tx", err))?;

        tx.execute("DELETE FROM fnn_channels", [])
            .map_err(|err| sanitize_storage_error("clear fnn_channels", err))?;

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
            .map_err(|err| sanitize_storage_error("insert channel snapshot", err))?;
        }

        tx.commit()
            .map_err(|err| sanitize_storage_error("commit channel snapshot tx", err))?;
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
            .map_err(|err| sanitize_storage_error("prepare list fnn_channels", err))?;

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
            .map_err(|err| sanitize_storage_error("query fnn_channels", err))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| sanitize_storage_error("collect fnn_channels", err))
    }

    pub fn dashboard_ledger_counts(&self) -> Result<DashboardLedgerCounts, String> {
        let conn = self.lock_conn()?;
        let scalar = |sql: &str| -> Result<u64, String> {
            conn.query_row(sql, [], |row| row.get::<_, i64>(0))
                .map(|value| value.max(0) as u64)
                .map_err(|err| sanitize_storage_error("dashboard count query failed", err))
        };

        Ok(DashboardLedgerCounts {
            dicoba_contributions: scalar(
                "SELECT COUNT(*) FROM dicoba_contributions",
            )?,
            dicoba_vaults_total: scalar(
                "SELECT COUNT(DISTINCT group_name) FROM dicoba_contributions WHERE group_name != ''",
            )?,
            edge_pending: scalar(
                "SELECT COUNT(*) FROM edge_transaction_ledger WHERE status = 'PENDING'",
            )?,
            edge_settled: scalar(
                "SELECT COUNT(*) FROM edge_transaction_ledger WHERE status = 'SETTLED'",
            )?,
            edge_failed: scalar(
                "SELECT COUNT(*) FROM edge_transaction_ledger WHERE status = 'FAILED'",
            )?,
            fiat_edge_transactions: scalar("SELECT COUNT(*) FROM fiat_edge_ledger")?,
            queued_telemetry: scalar("SELECT COUNT(*) FROM offline_telemetry_queue")?,
            cached_channels: scalar("SELECT COUNT(*) FROM fnn_channels")?,
        })
    }

    pub fn record_dicoba_contribution(
        &self,
        receipt: &mesh_core::jungukuu_types::MicroContributionReceipt,
        group_name: &str,
    ) -> Result<(), String> {
        let conn = self.lock_conn()?;
        conn.execute(
            r#"
            INSERT INTO dicoba_contributions
                (tx_id, vault_id, group_name, member_id, amount_shannons, timestamp, synced)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)
            "#,
            params![
                receipt.transaction_id.to_string(),
                receipt.vault_id.to_string(),
                group_name,
                receipt.member_id.to_string(),
                receipt.amount_shannons,
                receipt.timestamp,
            ],
        )
        .map_err(|err| sanitize_storage_error("record dicoba contribution", err))?;
        Ok(())
    }

    pub fn list_vault_contributors(
        &self,
        vault_id: &str,
        group_name: &str,
    ) -> Result<Vec<VaultContributorSummary>, String> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT
                    member_id,
                    COUNT(*) AS contribution_count,
                    COALESCE(SUM(amount_shannons), 0) AS total_shannons,
                    COALESCE(MAX(timestamp), 0) AS last_timestamp
                FROM dicoba_contributions
                WHERE vault_id = ?1 OR group_name = ?2
                GROUP BY member_id
                ORDER BY total_shannons DESC, last_timestamp DESC
                "#,
            )
            .map_err(|err| sanitize_storage_error("prepare vault contributors", err))?;

        let rows = stmt
            .query_map(params![vault_id, group_name], |row| {
                Ok(VaultContributorSummary {
                    member_id: row.get(0)?,
                    contribution_count: row.get::<_, i64>(1)? as u64,
                    total_shannons: row.get::<_, i64>(2)? as u64,
                    last_timestamp: row.get::<_, i64>(3)? as u64,
                })
            })
            .map_err(|err| sanitize_storage_error("query vault contributors", err))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| sanitize_storage_error("collect vault contributors", err))
    }

    pub fn list_member_vaults(&self, member_id: &str) -> Result<Vec<MemberVaultSummary>, String> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT
                    group_name,
                    vault_id,
                    COUNT(*) AS contribution_count,
                    COALESCE(SUM(amount_shannons), 0) AS total_shannons,
                    COALESCE(MAX(timestamp), 0) AS last_timestamp
                FROM dicoba_contributions
                WHERE member_id = ?1 AND group_name != ''
                GROUP BY group_name, vault_id
                ORDER BY last_timestamp DESC, group_name ASC
                "#,
            )
            .map_err(|err| sanitize_storage_error("prepare member vaults", err))?;

        let rows = stmt
            .query_map(params![member_id], |row| {
                Ok(MemberVaultSummary {
                    group_name: row.get(0)?,
                    vault_id: row.get(1)?,
                    contribution_count: row.get::<_, i64>(2)? as u64,
                    total_shannons: row.get::<_, i64>(3)? as u64,
                    last_timestamp: row.get::<_, i64>(4)? as u64,
                })
            })
            .map_err(|err| sanitize_storage_error("query member vaults", err))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| sanitize_storage_error("collect member vaults", err))
    }

    pub fn enqueue_telemetry(
        &self,
        event_type: &str,
        payload: &MeshPulsePayload,
    ) -> Result<i64, String> {
        let payload = serde_json::to_string(payload).map_err(|err| sanitize_storage_error("encode telemetry", err))?;
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
        .map_err(|err| sanitize_storage_error("enqueue telemetry", err))?;
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
            .map_err(|err| sanitize_storage_error("prepare dequeue telemetry", err))?;

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
            .map_err(|err| sanitize_storage_error("query dequeue telemetry", err))?;

        let item = match rows.next() {
            Some(Ok(item)) => item,
            Some(Err(err)) => return Err(sanitize_storage_error("read queued telemetry", err)),
            None => return Ok(None),
        };

        conn.execute(
            "DELETE FROM offline_telemetry_queue WHERE id = ?1",
            params![item.id],
        )
        .map_err(|err| sanitize_storage_error("delete queued telemetry", err))?;

        Ok(Some(item))
    }

    pub fn increment_telemetry_retry(&self, id: i64) -> Result<(), String> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE offline_telemetry_queue SET retry_count = retry_count + 1 WHERE id = ?1",
            params![id],
        )
        .map_err(|err| sanitize_storage_error("increment telemetry retry {id}", err))?;
        Ok(())
    }

    pub fn pending_telemetry_count(&self) -> Result<i64, String> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT COUNT(*) FROM offline_telemetry_queue",
            [],
            |row| row.get(0),
        )
        .map_err(|err| sanitize_storage_error("count telemetry queue", err))
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
        .map_err(|err| sanitize_storage_error("insert edge transaction", err))?;
        Ok(())
    }

    pub fn insert_fiat_edge_transaction(&self, tx: &EdgeTransaction) -> Result<(), String> {
        let conn = self.lock_conn()?;
        insert_fiat_edge_on_conn(&conn, tx)
    }

    pub fn fetch_next_pending_utility_intent(&self) -> Result<Option<UtilityPaymentIntent>, String> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, payment_hash, amount_shannons, status, synced
                FROM utility_payment_intents
                WHERE status = 'pending'
                ORDER BY id ASC
                LIMIT 1
                "#,
            )
            .map_err(|err| sanitize_storage_error("prepare utility intent query", err))?;

        let mut rows = stmt
            .query_map([], |row| {
                Ok(UtilityPaymentIntent {
                    id: row.get(0)?,
                    payment_hash: row.get(1)?,
                    amount_shannons: row.get::<_, i64>(2)? as u64,
                    status: row.get(3)?,
                    synced: row.get::<_, i32>(4)? != 0,
                })
            })
            .map_err(|err| sanitize_storage_error("query utility intents", err))?;

        rows.next().transpose().map_err(|err| sanitize_storage_error("read utility intent", err))
    }

    pub fn update_utility_intent_status(&self, id: i64, status: &str) -> Result<(), String> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE utility_payment_intents SET status = ?1 WHERE id = ?2",
            params![status, id],
        )
        .map_err(|err| sanitize_storage_error("update utility intent {id}", err))?;
        Ok(())
    }

    /// Returns true when the local fiat edge ledger records a settled payment for the hash.
    pub fn fiat_ledger_confirms_payment(
        &self,
        payment_hash: &str,
        min_amount_shannons: u64,
    ) -> Result<bool, String> {
        let conn = self.lock_conn()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fiat_edge_ledger WHERE payment_hash = ?1 AND amount_atomic >= ?2",
                params![payment_hash, min_amount_shannons as i64],
                |row| row.get(0),
            )
            .map_err(|err| sanitize_storage_error("query fiat ledger for payment hash", err))?;
        Ok(count > 0)
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
        .map_err(|err| sanitize_storage_error("update edge tx {tx_hash}", err))?;
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
            .map_err(|err| sanitize_storage_error("prepare list edge tx", err))?;

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
            .map_err(|err| sanitize_storage_error("query edge tx", err))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| sanitize_storage_error("collect edge tx", err))
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
            .map_err(|err| sanitize_storage_error("evict stale telemetry", err))?;

        let failed_pending_tx = conn
            .execute(
                r#"
                UPDATE edge_transaction_ledger
                SET status = 'FAILED'
                WHERE status = 'PENDING' AND created_at < datetime('now', ?1)
                "#,
                params![format!("-{hours} hours")],
            )
            .map_err(|err| sanitize_storage_error("fail stale pending transactions", err))?;

        Ok(MaintenanceReport {
            evicted_telemetry,
            failed_pending_tx,
        })
    }

    pub fn get_telco_float_record(&self, account_id: &str) -> Result<TelcoFloatSnapshot, String> {
        let trimmed = account_id.trim();
        if trimmed.is_empty() || trimmed.len() > 128 {
            return Err("account_id must be 1..=128 characters".to_string());
        }

        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT provider, account_id, live_balance_units, critical_floor_units
                FROM telco_float_accounts
                WHERE account_id = ?1
                "#,
            )
            .map_err(|err| sanitize_storage_error("prepare telco float query", err))?;

        match stmt.query_row(params![trimmed], |row| {
            Ok(TelcoFloatSnapshot {
                provider: row.get(0)?,
                account_id: row.get(1)?,
                live_balance_units: row.get::<_, i64>(2)? as u64,
                critical_floor_units: row.get::<_, i64>(3)? as u64,
            })
        }) {
            Ok(record) => Ok(record),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(TelcoFloatSnapshot {
                provider: String::new(),
                account_id: trimmed.to_string(),
                live_balance_units: 0,
                critical_floor_units: 0,
            }),
            Err(err) => Err(sanitize_storage_error("query telco float record", err)),
        }
    }

    pub fn increment_local_fiat_float(
        &self,
        account_id: &str,
        amount_units: u64,
    ) -> Result<TelcoFloatSnapshot, String> {
        let trimmed = account_id.trim();
        if trimmed.is_empty() || trimmed.len() > 128 {
            return Err("account_id must be 1..=128 characters".to_string());
        }

        {
            let conn = self.lock_conn()?;
            conn.execute(
                r#"
                INSERT INTO telco_float_accounts (account_id, provider, live_balance_units, critical_floor_units)
                VALUES (?1, '', 0, 0)
                ON CONFLICT(account_id) DO NOTHING
                "#,
                params![trimmed],
            )
            .map_err(|err| sanitize_storage_error("ensure telco float account", err))?;

            conn.execute(
                r#"
                UPDATE telco_float_accounts
                SET live_balance_units = live_balance_units + ?2,
                    updated_at = CURRENT_TIMESTAMP
                WHERE account_id = ?1
                "#,
                params![trimmed, amount_units as i64],
            )
            .map_err(|err| sanitize_storage_error("increment telco float balance", err))?;
        }

        self.get_telco_float_record(trimmed)
    }

    pub fn install_module(
        &self,
        module_name: &str,
        is_active: bool,
        config_json: &str,
    ) -> Result<InstalledModuleRecord, String> {
        let normalized = module_name.trim();
        if normalized.is_empty() || normalized.len() > 64 {
            return Err("module_name must be 1..=64 characters".to_string());
        }
        let id = uuid::Uuid::new_v4().to_string();
        {
            let conn = self.lock_conn()?;
            conn.execute(
                r#"
                INSERT INTO installed_modules (id, module_name, is_active, config_json)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(module_name) DO UPDATE SET
                    is_active = excluded.is_active,
                    config_json = excluded.config_json
                "#,
                params![id, normalized, is_active as i32, config_json],
            )
            .map_err(|err| sanitize_storage_error("install module", err))?;
        }
        self.fetch_installed_module(normalized)
    }

    pub fn uninstall_module(&self, module_name: &str) -> Result<bool, String> {
        let normalized = module_name.trim();
        let conn = self.lock_conn()?;
        let removed = conn
            .execute(
                "DELETE FROM installed_modules WHERE module_name = ?1",
                params![normalized],
            )
            .map_err(|err| sanitize_storage_error("uninstall module", err))?;
        Ok(removed > 0)
    }

    pub fn set_module_active_state(
        &self,
        module_name: &str,
        is_active: bool,
    ) -> Result<InstalledModuleRecord, String> {
        let normalized = module_name.trim();
        let updated = {
            let conn = self.lock_conn()?;
            conn.execute(
                "UPDATE installed_modules SET is_active = ?2 WHERE module_name = ?1",
                params![normalized, is_active as i32],
            )
            .map_err(|err| sanitize_storage_error("toggle module active state", err))?
        };
        if updated == 0 {
            return Err(format!("module '{normalized}' is not installed"));
        }
        self.fetch_installed_module(normalized)
    }

    pub fn get_installed_modules(&self) -> Result<Vec<InstalledModuleRecord>, String> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, module_name, is_active, config_json FROM installed_modules ORDER BY module_name",
            )
            .map_err(|err| sanitize_storage_error("prepare installed modules query", err))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(InstalledModuleRecord {
                    id: row.get(0)?,
                    module_name: row.get(1)?,
                    is_active: row.get::<_, i32>(2)? != 0,
                    config_json: row.get(3)?,
                })
            })
            .map_err(|err| sanitize_storage_error("query installed modules", err))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| sanitize_storage_error("read installed modules", err))
    }

    pub fn get_active_modules(&self) -> Result<Vec<InstalledModuleRecord>, String> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, module_name, is_active, config_json FROM installed_modules WHERE is_active = 1 ORDER BY module_name",
            )
            .map_err(|err| sanitize_storage_error("prepare active modules query", err))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(InstalledModuleRecord {
                    id: row.get(0)?,
                    module_name: row.get(1)?,
                    is_active: true,
                    config_json: row.get(3)?,
                })
            })
            .map_err(|err| sanitize_storage_error("query active modules", err))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| sanitize_storage_error("read active modules", err))
    }

    const REGISTRY_BOOTSTRAPPED_KEY: &'static str = "module_registry_bootstrapped";

    pub fn is_module_registry_bootstrapped(&self) -> Result<bool, String> {
        let conn = self.lock_conn()?;
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM sidecar_registry_meta WHERE key = ?1",
                params![Self::REGISTRY_BOOTSTRAPPED_KEY],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| sanitize_storage_error("read module registry bootstrap flag", err))?;
        Ok(matches!(value.as_deref(), Some("1")))
    }

    pub fn mark_module_registry_bootstrapped(&self) -> Result<(), String> {
        let conn = self.lock_conn()?;
        conn.execute(
            r#"
            INSERT INTO sidecar_registry_meta (key, value)
            VALUES (?1, '1')
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            params![Self::REGISTRY_BOOTSTRAPPED_KEY],
        )
        .map_err(|err| sanitize_storage_error("mark module registry bootstrapped", err))?;
        Ok(())
    }

    fn fetch_installed_module(&self, module_name: &str) -> Result<InstalledModuleRecord, String> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT id, module_name, is_active, config_json FROM installed_modules WHERE module_name = ?1",
            params![module_name],
            |row| {
                Ok(InstalledModuleRecord {
                    id: row.get(0)?,
                    module_name: row.get(1)?,
                    is_active: row.get::<_, i32>(2)? != 0,
                    config_json: row.get(3)?,
                })
            },
        )
        .map_err(|err| sanitize_storage_error("fetch installed module", err))
    }

    /// Loads the persisted MFA-issued identity, if present.
    pub fn load_agent_identity(&self) -> Result<Option<StoredAgentIdentity>, String> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT agent_id, agent_id_numeric, agent_secret FROM agent_identity LIMIT 1",
            [],
            |row| {
                Ok(StoredAgentIdentity {
                    agent_id: row.get(0)?,
                    agent_id_numeric: row.get::<_, i64>(1)? as u16,
                    agent_secret: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|err| sanitize_storage_error("load agent identity", err))
    }

    /// Replaces any prior MFA identity with the issued FA-N + HMAC secret.
    pub fn save_agent_identity(
        &self,
        agent_id: &str,
        agent_id_numeric: u16,
        agent_secret: &str,
    ) -> Result<(), String> {
        let conn = self.lock_conn()?;
        conn.execute("DELETE FROM agent_identity", [])
            .map_err(|err| sanitize_storage_error("clear agent identity", err))?;
        conn.execute(
            "INSERT INTO agent_identity (agent_id, agent_id_numeric, agent_secret) VALUES (?1, ?2, ?3)",
            params![agent_id, agent_id_numeric as i64, agent_secret],
        )
        .map_err(|err| sanitize_storage_error("save agent identity", err))?;
        Ok(())
    }

    fn lock_conn(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.conn
            .lock()
            .map_err(|_| "database mutex poisoned".to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAgentIdentity {
    pub agent_id: String,
    pub agent_id_numeric: u16,
    pub agent_secret: String,
}

fn insert_fiat_edge_on_conn(conn: &Connection, tx: &EdgeTransaction) -> Result<(), String> {
    let tx_type = match tx.tx_type {
        mesh_core::types::EdgeTxType::CashIn => "CASH_IN",
        mesh_core::types::EdgeTxType::CashOut => "CASH_OUT",
    };
    let primary = tx
        .primary_capacity()
        .ok_or_else(|| "edge transaction requires at least one asset capacity".to_string())?;
    let asset = primary.asset.ledger_label();
    let amount_atomic = tx.total_atomic();
    let capacities_json = serde_json::to_string(&tx.capacities)
        .map_err(|err| format!("serialize asset capacities: {err}"))?;
    conn.execute(
        r#"
        INSERT INTO fiat_edge_ledger (
            tx_id, agent_id, tx_type, asset, amount_atomic, asset_capacities_json,
            fiat_amount, counterparty_pubkey, payment_hash, preimage, timestamp, is_synchronized
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
        params![
            tx.tx_id.to_string(),
            tx.agent_id,
            tx_type,
            asset,
            amount_atomic as i64,
            capacities_json,
            tx.fiat_amount,
            tx.counterparty_pubkey,
            tx.payment_hash,
            tx.preimage,
            tx.timestamp,
            i32::from(tx.is_synchronized),
        ],
    )
    .map_err(|err| sanitize_storage_error("insert fiat edge transaction", err))?;
    Ok(())
}

fn init_single_writer_connection(db_path: &Path) -> Connection {
    let conn = open_performance_tuned_db(db_path).expect("Failed to initialize single-writer DB storage");
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA temp_store = MEMORY;
        ",
    )
    .expect("Failed to tune database pragmas");
    conn.execute_batch(SCHEMA_SQL)
        .expect("Failed to migrate single-writer schema");
    migrate_dicoba_contributions_columns(&conn).expect("Failed to migrate dicoba columns");
    migrate_fiat_edge_capacities_column(&conn).expect("Failed to migrate fiat edge capacities");
    migrate_module_registry_meta(&conn).expect("Failed to migrate module registry meta");
    migrate_agent_identity_table(&conn).expect("Failed to migrate agent identity");
    init_offline_ledger(&conn).expect("Failed to init offline ledger");
    conn
}

pub enum DbWriteCommand {
    InsertLedger {
        tx_id: String,
        payload: String,
        resp: oneshot::Sender<Result<(), MeshError>>,
    },
}

/// Serialized fiat-ledger writes on a dedicated native thread (single SQLite writer).
pub struct AsyncDbQueue {
    sender: mpsc::Sender<DbWriteCommand>,
}

impl AsyncDbQueue {
    pub fn new(db_path: PathBuf, queue_capacity: usize) -> Self {
        let (tx, mut rx) = mpsc::channel::<DbWriteCommand>(queue_capacity);

        std::thread::Builder::new()
            .name("fa-db-writer".into())
            .spawn(move || {
                let conn = init_single_writer_connection(&db_path);

                while let Some(cmd) = rx.blocking_recv() {
                    match cmd {
                        DbWriteCommand::InsertLedger {
                            tx_id,
                            payload,
                            resp,
                        } => {
                            let res = (|| {
                                let tx: EdgeTransaction = serde_json::from_str(&payload)
                                    .map_err(|err| {
                                        MeshError::StorageError(sanitize_storage_error(
                                            "decode fiat ledger payload",
                                            err,
                                        ))
                                    })?;
                                if tx.tx_id.to_string() != tx_id {
                                    return Err(MeshError::StorageError(
                                        "fiat ledger tx_id does not match payload".into(),
                                    ));
                                }
                                insert_fiat_edge_on_conn(&conn, &tx).map_err(|err| {
                                    MeshError::StorageError(err)
                                })
                            })();
                            let _ = resp.send(res);
                        }
                    }
                }
            })
            .expect("Failed to spawn single-writer DB thread");

        Self { sender: tx }
    }

    pub fn for_agent(agent_id: u16, queue_capacity: usize) -> Self {
        Self::new(resolve_db_path(agent_id), queue_capacity)
    }

    pub async fn push_ledger_entry(
        &self,
        tx_id: String,
        json_payload: String,
    ) -> Result<(), MeshError> {
        let (tx, rx) = oneshot::channel();

        self.sender
            .send(DbWriteCommand::InsertLedger {
                tx_id,
                payload: json_payload,
                resp: tx,
            })
            .await
            .map_err(|_| MeshError::StorageError("DB channel queue disconnected".into()))?;

        rx.await
            .map_err(|_| MeshError::StorageError("DB worker thread panicked".into()))?
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

/// Shared sidecar bootstrap DB for MFA-issued `FA-N` + HMAC secret (before agent id is known).
pub fn resolve_identity_db_path() -> PathBuf {
    if let Ok(path) = env::var("FIBER_AGENT_IDENTITY_DB_PATH") {
        return PathBuf::from(path);
    }
    let dir = env::var("FIBER_AGENT_STATE_DIR").unwrap_or_else(|_| DEFAULT_STATE_DIR.to_string());
    PathBuf::from(dir).join("agent_identity.db")
}

fn retention_hours() -> i64 {
    env::var("FIBER_AGENT_RETENTION_HOURS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|hours| *hours > 0)
        .unwrap_or(DEFAULT_RETENTION_HOURS)
}

fn migrate_module_registry_meta(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS sidecar_registry_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
    .map_err(|err| sanitize_storage_error("migrate sidecar_registry_meta", err))?;

    let user_version: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|err| sanitize_storage_error("read user_version", err))?;

    if user_version < 3 {
        let module_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM installed_modules", [], |row| row.get(0))
            .map_err(|err| sanitize_storage_error("count installed modules", err))?;
        let ledger_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edge_transaction_ledger",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Existing sidecar DBs (already used or previously seeded) must not profile-reseed
        // when the user clears every module via App Store.
        if user_version > 0 || module_count > 0 || ledger_rows > 0 {
            conn.execute(
                r#"
                INSERT INTO sidecar_registry_meta (key, value)
                VALUES ('module_registry_bootstrapped', '1')
                ON CONFLICT(key) DO UPDATE SET value = excluded.value
                "#,
                [],
            )
            .map_err(|err| sanitize_storage_error("mark legacy registry bootstrapped", err))?;
        }

        conn.pragma_update(None, "user_version", 3)
            .map_err(|err| sanitize_storage_error("bump user_version", err))?;
    }

    Ok(())
}

fn migrate_agent_identity_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS agent_identity (
            agent_id TEXT NOT NULL,
            agent_id_numeric INTEGER NOT NULL,
            agent_secret TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )
    .map_err(|err| sanitize_storage_error("migrate agent_identity", err))?;
    Ok(())
}

fn migrate_fiat_edge_capacities_column(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(fiat_edge_ledger)")
        .map_err(|err| sanitize_storage_error("pragma fiat_edge_ledger", err))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|err| sanitize_storage_error("read fiat_edge_ledger columns", err))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| sanitize_storage_error("collect fiat_edge_ledger columns", err))?;

    if !columns.iter().any(|name| name == "asset_capacities_json") {
        conn.execute(
            "ALTER TABLE fiat_edge_ledger ADD COLUMN asset_capacities_json TEXT",
            [],
        )
        .map_err(|err| sanitize_storage_error("add asset_capacities_json column", err))?;
    }
    Ok(())
}

fn migrate_dicoba_contributions_columns(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(dicoba_contributions)")
        .map_err(|err| sanitize_storage_error("pragma dicoba_contributions", err))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|err| sanitize_storage_error("read dicoba columns", err))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| sanitize_storage_error("collect dicoba columns", err))?;

    if !columns.iter().any(|name| name == "group_name") {
        conn.execute(
            "ALTER TABLE dicoba_contributions ADD COLUMN group_name TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(|err| sanitize_storage_error("add group_name column", err))?;
    }
    if !columns.iter().any(|name| name == "member_id") {
        conn.execute(
            "ALTER TABLE dicoba_contributions ADD COLUMN member_id TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(|err| sanitize_storage_error("add member_id column", err))?;
    }

    conn.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_dicoba_contributions_group ON dicoba_contributions(group_name);
        CREATE INDEX IF NOT EXISTS idx_dicoba_contributions_member ON dicoba_contributions(member_id);
        "#,
    )
    .map_err(|err| sanitize_storage_error("dicoba contributor indexes", err))?;
    Ok(())
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
            agent_id: 44,
            timestamp: 1,
            nonce: 1,
            local_capacity_shannons: 100,
            public_key_hex: None,
            signature_hex: None,
            status: "MESH_HEARTBEAT".to_string(),
            active_mesh_neighbors: vec![45],
            report_target: 44,
            attempt: 0,
            fnn_pubkey_hex: None,
            peer_connect_address: None,
            asset_capacities: Vec::new(),
        };
        let second = MeshPulsePayload {
            agent_id: 44,
            timestamp: 2,
            nonce: 2,
            local_capacity_shannons: 0,
            public_key_hex: None,
            signature_hex: None,
            status: "ALERT_MFA_NODE_DROPPED".to_string(),
            active_mesh_neighbors: vec![],
            report_target: 45,
            attempt: 1,
            fnn_pubkey_hex: None,
            peer_connect_address: None,
            asset_capacities: Vec::new(),
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
        let conn = open_performance_tuned_db(&path).expect("open wal db");
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("read journal_mode");
        assert_eq!(mode.to_ascii_lowercase(), "wal");
        drop(conn);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn fetch_next_pending_utility_intent_returns_oldest_pending() {
        let path = std::env::temp_dir().join(format!(
            "fiber-agent-utility-intent-{}.db",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        {
            let file_conn = Connection::open(&path).expect("open file db");
            init_offline_ledger(&file_conn).expect("init");
            file_conn
                .execute(
                    "INSERT INTO utility_payment_intents (id, payment_hash, amount_shannons, status)
                     VALUES (2, '0xbbb', 2000, 'pending'), (1, '0xaaa', 1000, 'pending')",
                    [],
                )
                .expect("seed");
        }

        let db = AgentDb::open_path(path.clone()).expect("open db");
        let intent = db
            .fetch_next_pending_utility_intent()
            .expect("fetch")
            .expect("pending intent");
        assert_eq!(intent.id, 1);
        assert_eq!(intent.payment_hash, "0xaaa");

        db.update_utility_intent_status(1, "confirmed")
            .expect("update status");
        let next = db
            .fetch_next_pending_utility_intent()
            .expect("fetch")
            .expect("second pending");
        assert_eq!(next.id, 2);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn init_offline_ledger_creates_utility_payment_intents_table() {
        let conn = Connection::open_in_memory().expect("memory db");
        init_offline_ledger(&conn).expect("init offline ledger");

        conn.execute(
            "INSERT INTO utility_payment_intents (id, payment_hash, amount_shannons, status)
             VALUES (1, '0xabc', 50000, 'pending')",
            [],
        )
        .expect("insert intent");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM utility_payment_intents WHERE status = 'pending'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn resolve_db_path_uses_agent_suffix() {
        let path = resolve_db_path(77);
        assert!(path.to_string_lossy().contains("fa-77.db"));
    }

    #[test]
    fn agent_identity_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "fiber-agent-identity-store-{}.db",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let db = AgentDb::open_path(path.clone()).expect("open db");
        assert!(db.load_agent_identity().expect("load").is_none());
        db.save_agent_identity("FA-5", 5, &"b".repeat(64))
            .expect("save");
        let loaded = db.load_agent_identity().expect("load").expect("row");
        assert_eq!(loaded.agent_id, "FA-5");
        assert_eq!(loaded.agent_id_numeric, 5);
        assert_eq!(loaded.agent_secret.len(), 64);
        assert!(resolve_identity_db_path()
            .to_string_lossy()
            .contains("agent_identity.db"));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn async_db_queue_inserts_fiat_ledger_row() {
        use mesh_core::types::{EdgeTxType, L2Asset, SingleCapacityParams};
        use uuid::Uuid;

        let path = std::env::temp_dir().join(format!(
            "fiber-agent-async-queue-{}.db",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let db = AgentDb::open_path(path.clone()).expect("open db");
        let queue = AsyncDbQueue::new(path.clone(), 8);

        let tx = EdgeTransaction::single_capacity(SingleCapacityParams {
            tx_id: Uuid::new_v4(),
            agent_id: 44,
            tx_type: EdgeTxType::CashIn,
            asset: L2Asset::CkbNative,
            amount_atomic: 250_000,
            fiat_amount: 12.5,
            counterparty_pubkey: agent_fnn_pubkey(45),
            payment_hash: Some("0xasync-queue-hash".to_string()),
            preimage: None,
            timestamp: Utc::now().timestamp(),
            is_synchronized: false,
        });
        let tx_id = tx.tx_id.to_string();
        let payload = serde_json::to_string(&tx).expect("serialize edge tx");

        queue
            .push_ledger_entry(tx_id, payload)
            .await
            .expect("async ledger insert");

        let counts = db.dashboard_ledger_counts().expect("counts");
        assert_eq!(counts.fiat_edge_transactions, 1);
        assert!(db
            .fiat_ledger_confirms_payment("0xasync-queue-hash", 250_000)
            .expect("confirm"));

        let _ = std::fs::remove_file(path);
    }
}
