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
    Ok(())
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
}
