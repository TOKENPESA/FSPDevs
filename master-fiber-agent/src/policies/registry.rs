//! Dynamic hot-swapping plugin registry for MFA policy + clearing plugins.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::config::telco_clearing_api_url;
use crate::mfa_storage::{InstalledModuleRecord, MfaModuleStore};
use crate::plugin_registry::PluginRegistry;
use crate::plugins::{
    automated_refueling::AutomatedRefuelingBrain, clearinghouse_swap::ClearinghouseSwapModule,
    lume_pricing::LumePricingEngine, sovereign_compliance::SovereignComplianceFilter,
};
use crate::traits::{MfaClearingPlugin, MfaPolicyPlugin};

pub struct PluginHotReloader {
    pub registry: PluginRegistry,
    store: Arc<MfaModuleStore>,
    critical_capacity_floor: u64,
}

impl PluginHotReloader {
    pub fn new(registry: PluginRegistry, store: Arc<MfaModuleStore>, critical_capacity_floor: u64) -> Self {
        Self {
            registry,
            store,
            critical_capacity_floor,
        }
    }

    pub async fn hydrate_from_storage(&self) -> Result<(), String> {
        let installed = self.store.get_installed_modules()?;
        if installed.is_empty() && !self.store.is_plugin_registry_bootstrapped()? {
            self.seed_defaults()?;
            self.store.mark_plugin_registry_bootstrapped()?;
        }
        let installed = self.store.get_installed_modules()?;
        for record in installed {
            if record.is_active {
                let config: Value =
                    serde_json::from_str(&record.config_json).unwrap_or_else(|_| json!({}));
                self.mount_module(&record.module_name, config).await?;
            }
        }
        Ok(())
    }

    fn seed_defaults(&self) -> Result<(), String> {
        for plugin_id in crate::policies::catalog::KNOWN_PLUGIN_IDS {
            self.store
                .install_module(plugin_id, true, "{}")
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub async fn mount_module(&self, module_name: &str, config: Value) -> Result<(), String> {
        let plugin_id = crate::policies::catalog::normalize_plugin_id(module_name)
            .ok_or_else(|| format!("Unknown plugin '{module_name}'"))?;
        let plugin = build_policy_plugin(plugin_id, &config, self.critical_capacity_floor)?;
        if let Some(plugin) = plugin {
            self.registry.mount_policy_plugin(plugin_id, plugin, true).await;
            return Ok(());
        }
        let clearing = build_clearing_plugin(plugin_id, &config)?;
        if let Some(clearing) = clearing {
            self.registry.mount_clearing_plugin(clearing).await;
            return Ok(());
        }
        Err(format!("Plugin '{plugin_id}' is not in the native catalog"))
    }

    pub async fn unmount_module(&self, module_name: &str) -> Result<(), String> {
        let plugin_id = crate::policies::catalog::normalize_plugin_id(module_name)
            .ok_or_else(|| format!("Unknown plugin '{module_name}'"))?;
        if self.registry.unmount_plugin(plugin_id).await {
            Ok(())
        } else {
            Err(format!("Plugin '{plugin_id}' is not mounted"))
        }
    }

    pub async fn install_and_mount(
        &self,
        module_name: &str,
        config: Value,
    ) -> Result<InstalledModuleRecord, String> {
        let plugin_id = crate::policies::catalog::normalize_plugin_id(module_name)
            .ok_or_else(|| format!("Unknown plugin '{module_name}'"))?;
        let config_json =
            serde_json::to_string(&config).map_err(|e| format!("config serialization failed: {e}"))?;
        let record = self
            .store
            .install_module(plugin_id, true, &config_json)?;
        self.store.mark_plugin_registry_bootstrapped()?;
        self.mount_module(plugin_id, config).await?;
        Ok(record)
    }

    pub async fn uninstall(&self, module_name: &str) -> Result<(), String> {
        let plugin_id = crate::policies::catalog::normalize_plugin_id(module_name)
            .ok_or_else(|| format!("Unknown plugin '{module_name}'"))?;
        let _ = self.registry.unmount_plugin(plugin_id).await;
        if !self.store.uninstall_module(plugin_id)? {
            return Err(format!("Plugin '{plugin_id}' is not installed"));
        }
        self.store.mark_plugin_registry_bootstrapped()?;
        Ok(())
    }

    pub async fn toggle(
        &self,
        module_name: &str,
        active: bool,
    ) -> Result<InstalledModuleRecord, String> {
        let plugin_id = crate::policies::catalog::normalize_plugin_id(module_name)
            .ok_or_else(|| format!("Unknown plugin '{module_name}'"))?;
        let record = self.store.set_module_active_state(plugin_id, active)?;
        if active {
            let config: Value =
                serde_json::from_str(&record.config_json).unwrap_or_else(|_| json!({}));
            self.mount_module(plugin_id, config).await?;
        } else if plugin_id == "clearinghouse_swap" {
            let _ = self.registry.unmount_plugin(plugin_id).await;
        } else {
            self.registry.set_plugin_active(plugin_id, false).await;
        }
        Ok(record)
    }
}

fn build_policy_plugin(
    plugin_id: &str,
    config: &Value,
    default_floor: u64,
) -> Result<Option<Arc<dyn MfaPolicyPlugin>>, String> {
    let plugin: Arc<dyn MfaPolicyPlugin> = match plugin_id {
        "lume_pricing" => Arc::new(LumePricingEngine::new()),
        "sovereign_compliance" => Arc::new(SovereignComplianceFilter::new()),
        "automated_refueling" => {
            let floor = config
                .get("critical_capacity_floor")
                .and_then(Value::as_u64)
                .unwrap_or(default_floor);
            Arc::new(AutomatedRefuelingBrain::new(floor))
        }
        "clearinghouse_swap" => return Ok(None),
        other => return Err(format!("unknown policy plugin '{other}'")),
    };
    Ok(Some(plugin))
}

fn build_clearing_plugin(
    plugin_id: &str,
    config: &Value,
) -> Result<Option<Arc<dyn MfaClearingPlugin>>, String> {
    if plugin_id != "clearinghouse_swap" {
        return Ok(None);
    }
    let telco_url = config
        .get("telco_api_url")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(telco_clearing_api_url);
    Ok(Some(Arc::new(ClearinghouseSwapModule::new(telco_url))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hot_reloader_mounts_default_policy_plugins() {
        let path = std::env::temp_dir().join(format!(
            "mfa-hot-reload-{}.db",
            uuid::Uuid::new_v4()
        ));
        std::env::set_var("MFA_SUPERVISOR_DB_PATH", path.to_string_lossy().to_string());
        let store = Arc::new(MfaModuleStore::open().expect("open db"));
        let registry = PluginRegistry::empty();
        let reloader = PluginHotReloader::new(registry, store, 1_000_000);
        reloader.hydrate_from_storage().await.expect("hydrate");
        assert!(reloader.registry.has_plugin("lume_pricing").await);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn hydrate_does_not_reseed_after_user_uninstalls_all() {
        let path = std::env::temp_dir().join(format!(
            "mfa-hot-reload-clear-{}.db",
            uuid::Uuid::new_v4()
        ));
        std::env::set_var("MFA_SUPERVISOR_DB_PATH", path.to_string_lossy().to_string());
        let store = Arc::new(MfaModuleStore::open().expect("open db"));
        store
            .install_module("lume_pricing", true, "{}")
            .expect("install");
        store.mark_plugin_registry_bootstrapped().expect("bootstrap");

        let registry = PluginRegistry::empty();
        let reloader = PluginHotReloader::new(registry, store.clone(), 1_000_000);
        reloader
            .uninstall("lume_pricing")
            .await
            .expect("uninstall");
        reloader.hydrate_from_storage().await.expect("hydrate");
        assert!(!reloader.registry.has_plugin("lume_pricing").await);
        assert!(store.get_installed_modules().expect("installed").is_empty());
        let _ = std::fs::remove_file(path);
    }
}
