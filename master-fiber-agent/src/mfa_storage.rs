//! MFA supervisor plugin installation persistence (app store).

use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use crate::storage_error::sanitize_storage_error;
use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};
use serde::{Deserialize, Serialize};

const DEFAULT_STATE_DIR: &str = ".mfa-supervisor";

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS installed_modules (
    id TEXT PRIMARY KEY,
    module_name TEXT NOT NULL UNIQUE,
    is_active INTEGER NOT NULL DEFAULT 1,
    config_json TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_mfa_installed_modules_active ON installed_modules(is_active);

CREATE TABLE IF NOT EXISTS mfa_registry_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_telemetry_nonces (
    agent_id INTEGER PRIMARY KEY NOT NULL,
    high_water_mark INTEGER NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS registered_agents (
    agent_id INTEGER PRIMARY KEY NOT NULL,
    agent_secret TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledModuleRecord {
    pub id: String,
    pub module_name: String,
    pub is_active: bool,
    pub config_json: String,
}

pub fn resolve_mfa_db_path() -> PathBuf {
    if let Ok(path) = env::var("MFA_SUPERVISOR_DB_PATH") {
        return PathBuf::from(path);
    }
    let state_dir = env::var("MFA_STATE_DIR").unwrap_or_else(|_| DEFAULT_STATE_DIR.to_string());
    Path::new(&state_dir).join("mfa-supervisor.db")
}

fn open_db(path: &Path) -> SqliteResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                Some(format!("create MFA store directory: {err}")),
            )
        })?;
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;
    conn.execute_batch(SCHEMA_SQL)?;
    migrate_mfa_registry_meta(&conn)?;
    Ok(conn)
}

fn migrate_mfa_registry_meta(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS mfa_registry_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS agent_telemetry_nonces (
            agent_id INTEGER PRIMARY KEY NOT NULL,
            high_water_mark INTEGER NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )?;

    let user_version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if user_version < 2 {
        let module_count: i64 = conn.query_row("SELECT COUNT(*) FROM installed_modules", [], |row| {
            row.get(0)
        })?;
        if user_version > 0 || module_count > 0 {
            conn.execute(
                r#"
                INSERT INTO mfa_registry_meta (key, value)
                VALUES ('plugin_registry_bootstrapped', '1')
                ON CONFLICT(key) DO UPDATE SET value = excluded.value
                "#,
                [],
            )?;
        }
        conn.pragma_update(None, "user_version", 2)?;
    }
    let user_version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if user_version < 3 {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS agent_telemetry_nonces (
                agent_id INTEGER PRIMARY KEY NOT NULL,
                high_water_mark INTEGER NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            "#,
        )?;
        conn.pragma_update(None, "user_version", 3)?;
    }
    let user_version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if user_version < 4 {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS registered_agents (
                agent_id INTEGER PRIMARY KEY NOT NULL,
                agent_secret TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            "#,
        )?;
        conn.pragma_update(None, "user_version", 4)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisteredAgentRecord {
    pub agent_id: u16,
    pub agent_secret: String,
    pub created_at: String,
}

#[derive(Debug)]
pub struct MfaModuleStore {
    conn: Mutex<Connection>,
}

impl MfaModuleStore {
    pub fn open() -> Result<Self, String> {
        let path = resolve_mfa_db_path();
        let conn = open_db(&path).map_err(|err| sanitize_storage_error("open MFA module store", err))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
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
            .map_err(|err| sanitize_storage_error("toggle module", err))?
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
            .map_err(|err| sanitize_storage_error("prepare installed modules", err))?;
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
            .map_err(|err| sanitize_storage_error("prepare active modules", err))?;
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

    const REGISTRY_BOOTSTRAPPED_KEY: &'static str = "plugin_registry_bootstrapped";

    pub fn is_plugin_registry_bootstrapped(&self) -> Result<bool, String> {
        let conn = self.lock_conn()?;
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM mfa_registry_meta WHERE key = ?1",
                params![Self::REGISTRY_BOOTSTRAPPED_KEY],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| sanitize_storage_error("read plugin registry bootstrap flag", err))?;
        Ok(matches!(value.as_deref(), Some("1")))
    }

    pub fn mark_plugin_registry_bootstrapped(&self) -> Result<(), String> {
        let conn = self.lock_conn()?;
        conn.execute(
            r#"
            INSERT INTO mfa_registry_meta (key, value)
            VALUES (?1, '1')
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            params![Self::REGISTRY_BOOTSTRAPPED_KEY],
        )
        .map_err(|err| sanitize_storage_error("mark plugin registry bootstrapped", err))?;
        Ok(())
    }

    /// Loads all persisted telemetry nonce high-water marks (agent_id → HWM).
    pub fn load_telemetry_nonces(&self) -> Result<std::collections::HashMap<u16, u64>, String> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT agent_id, high_water_mark FROM agent_telemetry_nonces ORDER BY agent_id",
            )
            .map_err(|err| sanitize_storage_error("prepare telemetry nonces", err))?;
        let rows = stmt
            .query_map([], |row| {
                let agent_id: i64 = row.get(0)?;
                let hwm: i64 = row.get(1)?;
                Ok((agent_id as u16, hwm as u64))
            })
            .map_err(|err| sanitize_storage_error("query telemetry nonces", err))?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let (agent_id, hwm) =
                row.map_err(|err| sanitize_storage_error("read telemetry nonce row", err))?;
            map.insert(agent_id, hwm);
        }
        Ok(map)
    }

    /// Upserts a single agent's telemetry nonce high-water mark.
    pub fn upsert_telemetry_nonce(&self, agent_id: u16, high_water_mark: u64) -> Result<(), String> {
        let conn = self.lock_conn()?;
        conn.execute(
            r#"
            INSERT INTO agent_telemetry_nonces (agent_id, high_water_mark, updated_at)
            VALUES (?1, ?2, datetime('now'))
            ON CONFLICT(agent_id) DO UPDATE SET
                high_water_mark = excluded.high_water_mark,
                updated_at = excluded.updated_at
            WHERE excluded.high_water_mark > agent_telemetry_nonces.high_water_mark
            "#,
            params![agent_id as i64, high_water_mark as i64],
        )
        .map_err(|err| sanitize_storage_error("upsert telemetry nonce", err))?;
        Ok(())
    }

    /// Allocates the lowest free FA id in `min_agent_id..=max_agent_id` and persists a HMAC secret.
    pub fn register_agent(
        &self,
        min_agent_id: u16,
        max_agent_id: u16,
        agent_secret: &str,
    ) -> Result<RegisteredAgentRecord, String> {
        if min_agent_id == 0 || min_agent_id > max_agent_id {
            return Err("invalid agent id allocation range".to_string());
        }
        if agent_secret.len() < 32 {
            return Err("agent_secret must be at least 32 hex characters".to_string());
        }
        let conn = self.lock_conn()?;
        let mut used = std::collections::HashSet::new();
        {
            let mut stmt = conn
                .prepare("SELECT agent_id FROM registered_agents")
                .map_err(|err| sanitize_storage_error("prepare registered agents", err))?;
            let rows = stmt
                .query_map([], |row| row.get::<_, i64>(0))
                .map_err(|err| sanitize_storage_error("query registered agents", err))?;
            for row in rows {
                let id = row.map_err(|err| sanitize_storage_error("read registered agent id", err))?;
                used.insert(id as u16);
            }
        }
        let Some(agent_id) = (min_agent_id..=max_agent_id).find(|id| !used.contains(id)) else {
            return Err(format!(
                "mesh agent capacity exhausted ({min_agent_id}..={max_agent_id} registered)"
            ));
        };
        conn.execute(
            r#"
            INSERT INTO registered_agents (agent_id, agent_secret, created_at)
            VALUES (?1, ?2, datetime('now'))
            "#,
            params![agent_id as i64, agent_secret],
        )
        .map_err(|err| sanitize_storage_error("register agent", err))?;
        conn.query_row(
            "SELECT agent_id, agent_secret, created_at FROM registered_agents WHERE agent_id = ?1",
            params![agent_id as i64],
            |row| {
                Ok(RegisteredAgentRecord {
                    agent_id: row.get::<_, i64>(0)? as u16,
                    agent_secret: row.get(1)?,
                    created_at: row.get(2)?,
                })
            },
        )
        .map_err(|err| sanitize_storage_error("fetch registered agent", err))
    }

    /// Returns the HMAC secret for a previously registered agent, if any.
    pub fn get_registered_agent_secret(&self, agent_id: u16) -> Result<Option<String>, String> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT agent_secret FROM registered_agents WHERE agent_id = ?1",
            params![agent_id as i64],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| sanitize_storage_error("lookup registered agent secret", err))
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

    fn lock_conn(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.conn
            .lock()
            .map_err(|_| "MFA module store mutex poisoned".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_and_toggle_module_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "mfa-modules-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store = MfaModuleStore {
            conn: Mutex::new(open_db(&path).expect("open")),
        };
        let record = store
            .install_module("lume_pricing", true, r#"{"spread_bps":25}"#)
            .expect("install");
        assert_eq!(record.module_name, "lume_pricing");
        let toggled = store
            .set_module_active_state("lume_pricing", false)
            .expect("toggle");
        assert!(!toggled.is_active);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn telemetry_nonce_upsert_and_load() {
        let path = std::env::temp_dir().join(format!(
            "mfa-nonces-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store = MfaModuleStore {
            conn: Mutex::new(open_db(&path).expect("open")),
        };
        store.upsert_telemetry_nonce(1, 100).expect("upsert");
        store.upsert_telemetry_nonce(1, 50).expect("ignore lower");
        store.upsert_telemetry_nonce(1, 250).expect("raise");
        store.upsert_telemetry_nonce(7, 3).expect("second agent");
        let map = store.load_telemetry_nonces().expect("load");
        assert_eq!(map.get(&1), Some(&250));
        assert_eq!(map.get(&7), Some(&3));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn register_agent_allocates_lowest_free_id() {
        let path = std::env::temp_dir().join(format!(
            "mfa-register-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store = MfaModuleStore {
            conn: Mutex::new(open_db(&path).expect("open")),
        };
        let first = store
            .register_agent(1, 8, &"a".repeat(64))
            .expect("first");
        assert_eq!(first.agent_id, 1);
        let second = store
            .register_agent(1, 8, &"b".repeat(64))
            .expect("second");
        assert_eq!(second.agent_id, 2);
        assert_eq!(
            store.get_registered_agent_secret(2).expect("lookup").as_deref(),
            Some(second.agent_secret.as_str())
        );
        let _ = std::fs::remove_file(path);
    }
}
