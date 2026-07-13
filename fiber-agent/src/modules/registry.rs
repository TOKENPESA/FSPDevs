//! Dynamic hot-swapping module registry for runtime UI install/uninstall.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::module_catalog::{is_known_module_id, normalize_module_id};
use crate::module_registry::{build_module_with_config, SidecarBootContext};
use crate::module_system::SidecarModule;
use crate::storage::{AgentDb, InstalledModuleRecord};

pub struct ModuleSlot {
    pub module: Arc<dyn SidecarModule>,
    pub active: bool,
}

/// Thread-safe runtime module matrix (`Arc<RwLock<HashMap<…>>>`).
#[derive(Clone)]
pub struct DynamicModuleRegistry {
    modules: Arc<RwLock<HashMap<String, ModuleSlot>>>,
    outbound_tx: mpsc::Sender<mesh_core::network::PeerModulePacket>,
}

impl DynamicModuleRegistry {
    pub fn new(outbound_tx: mpsc::Sender<mesh_core::network::PeerModulePacket>) -> Self {
        Self {
            modules: Arc::new(RwLock::new(HashMap::new())),
            outbound_tx,
        }
    }

    pub async fn mounted_names(&self) -> Vec<String> {
        let guard = self.modules.read().expect("module registry lock");
        let mut names: Vec<String> = guard
            .iter()
            .filter(|(_, slot)| slot.active)
            .map(|(name, _)| name.clone())
            .collect();
        names.sort_unstable();
        names
    }

    pub async fn is_mounted(&self, module_id: &str) -> bool {
        let guard = self.modules.read().expect("module registry lock");
        guard
            .get(module_id)
            .map(|slot| slot.active)
            .unwrap_or(false)
    }

    pub async fn route_command(
        &self,
        target_module: &str,
        method: &str,
        payload: Value,
    ) -> Result<Value, String> {
        let module = {
            let guard = self.modules.read().expect("module registry lock");
            let slot = guard.get(target_module).ok_or_else(|| {
                format!("Module '{target_module}' is not registered on this sidecar.")
            })?;
            if !slot.active {
                return Err(format!("Module '{target_module}' is installed but paused."));
            }
            slot.module.clone()
        };
        module.handle_rpc_command(method, payload).await
    }

    pub async fn route_peer_message(
        &self,
        packet: mesh_core::network::PeerModulePacket,
    ) -> Result<(), String> {
        let module = {
            let guard = self.modules.read().expect("module registry lock");
            let slot = guard.get(packet.target_module.as_str()).ok_or_else(|| {
                format!(
                    "Dropped P2P packet: Module '{}' not installed.",
                    packet.target_module
                )
            })?;
            if !slot.active {
                return Err(format!(
                    "Dropped P2P packet: Module '{}' is paused.",
                    packet.target_module
                ));
            }
            slot.module.clone()
        };
        module
            .handle_peer_message(packet.source_agent_id, &packet.method, packet.payload)
            .await
    }

    pub fn generate_oob_fallback(
        &self,
        agent_id: u16,
        target_module: &str,
        target_agent: u16,
        method: &str,
        payload: Value,
    ) -> Result<String, String> {
        let guard = self.modules.read().expect("module registry lock");
        let slot = guard.get(target_module).ok_or_else(|| {
            format!("Module '{target_module}' is not registered on this sidecar.")
        })?;
        if !slot.active {
            return Err(format!("Module '{target_module}' is paused."));
        }
        let packet = slot
            .module
            .build_fallback_packet(target_agent, method, payload);
        let secret_key = crate::identity::resolve_agent_secret_key(agent_id)?;
        let signed = crate::peer_packet::sign_peer_module_packet(packet, &secret_key)?;
        signed.to_fallback_uri()
    }

    async fn insert_module(&self, module_id: String, module: Arc<dyn SidecarModule>, active: bool) {
        let mut guard = self.modules.write().expect("module registry lock");
        guard.insert(
            module_id,
            ModuleSlot {
                module,
                active,
            },
        );
    }

    async fn remove_module(&self, module_id: &str) -> bool {
        let mut guard = self.modules.write().expect("module registry lock");
        guard.remove(module_id).is_some()
    }

    async fn set_active(&self, module_id: &str, active: bool) -> Result<(), String> {
        let mut guard = self.modules.write().expect("module registry lock");
        let slot = guard
            .get_mut(module_id)
            .ok_or_else(|| format!("Module '{module_id}' is not mounted"))?;
        slot.active = active;
        Ok(())
    }

    pub(crate) async fn mount_instance(
        &self,
        module_id: &str,
        mut module: Box<dyn SidecarModule>,
        active: bool,
    ) -> Result<(), String> {
        module.set_outbound_channel(self.outbound_tx.clone());
        module.initialize().await?;
        self.insert_module(module_id.to_string(), Arc::from(module), active)
            .await;
        log::info!("🔌 [HOT-RELOAD] Mounted module: {module_id} (active={active})");
        Ok(())
    }

    pub(crate) async fn unmount_instance(&self, module_id: &str) -> Result<(), String> {
        if self.remove_module(module_id).await {
            log::info!("🔌 [HOT-RELOAD] Unmounted module: {module_id}");
            Ok(())
        } else {
            Err(format!("Module '{module_id}' is not mounted"))
        }
    }

    pub(crate) async fn toggle_instance(&self, module_id: &str, active: bool) -> Result<(), String> {
        self.set_active(module_id, active).await?;
        log::info!("🔌 [HOT-RELOAD] Module {module_id} active={active}");
        Ok(())
    }
}

/// Coordinates SQLite persistence with runtime mount/unmount.
pub struct HotReloader {
    pub registry: DynamicModuleRegistry,
    db: Arc<AgentDb>,
    factory_ctx: Arc<SidecarBootContext>,
}

impl HotReloader {
    pub fn new(
        registry: DynamicModuleRegistry,
        db: Arc<AgentDb>,
        factory_ctx: Arc<SidecarBootContext>,
    ) -> Self {
        Self {
            registry,
            db,
            factory_ctx,
        }
    }

    pub async fn hydrate_from_storage(&self) -> Result<(), String> {
        let installed = self.db.get_installed_modules()?;
        if installed.is_empty() && !self.db.is_module_registry_bootstrapped()? {
            self.seed_from_profile()?;
            self.db.mark_module_registry_bootstrapped()?;
        }
        let installed = self.db.get_installed_modules()?;
        for record in installed {
            if record.is_active {
                let config: Value =
                    serde_json::from_str(&record.config_json).unwrap_or_else(|_| json!({}));
                self.mount_module(&record.module_name, config).await?;
            }
        }
        Ok(())
    }

    fn seed_from_profile(&self) -> Result<(), String> {
        for module_id in self.factory_ctx.profile.enabled_module_ids() {
            self.db
                .install_module(module_id, true, "{}")
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub async fn mount_module(&self, module_name: &str, config: Value) -> Result<(), String> {
        let module_id = normalize_module_id(module_name)
            .ok_or_else(|| format!("Unknown module '{module_name}'"))?;
        if !is_known_module_id(module_id) {
            return Err(format!("Module '{module_id}' is not in the native catalog"));
        }
        if self.registry.is_mounted(module_id).await {
            self.registry.unmount_instance(module_id).await?;
        }
        let module = build_module_with_config(module_id, &self.factory_ctx, &config)?;
        self.registry
            .mount_instance(module_id, module, true)
            .await
    }

    pub async fn unmount_module(&self, module_name: &str) -> Result<(), String> {
        let module_id = normalize_module_id(module_name)
            .ok_or_else(|| format!("Unknown module '{module_name}'"))?;
        self.registry.unmount_instance(module_id).await
    }

    pub async fn install_and_mount(
        &self,
        module_name: &str,
        config: Value,
    ) -> Result<InstalledModuleRecord, String> {
        let module_id = normalize_module_id(module_name)
            .ok_or_else(|| format!("Unknown module '{module_name}'"))?;
        let config_json = serde_json::to_string(&config)
            .map_err(|e| format!("config serialization failed: {e}"))?;
        let record = self
            .db
            .install_module(module_id, true, &config_json)?;
        self.db.mark_module_registry_bootstrapped()?;
        self.mount_module(module_id, config).await?;
        Ok(record)
    }

    pub async fn uninstall(&self, module_name: &str) -> Result<(), String> {
        let module_id = normalize_module_id(module_name)
            .ok_or_else(|| format!("Unknown module '{module_name}'"))?;
        let _ = self.registry.unmount_instance(module_id).await;
        if !self.db.uninstall_module(module_id)? {
            return Err(format!("Module '{module_id}' is not installed"));
        }
        self.db.mark_module_registry_bootstrapped()?;
        Ok(())
    }

    pub async fn toggle(&self, module_name: &str, active: bool) -> Result<InstalledModuleRecord, String> {
        let module_id = normalize_module_id(module_name)
            .ok_or_else(|| format!("Unknown module '{module_name}'"))?;
        let record = self.db.set_module_active_state(module_id, active)?;
        if active {
            let config: Value =
                serde_json::from_str(&record.config_json).unwrap_or_else(|_| json!({}));
            if !self.registry.is_mounted(module_id).await {
                self.mount_module(module_id, config).await?;
            } else {
                self.registry.toggle_instance(module_id, true).await?;
            }
        } else if self.registry.is_mounted(module_id).await {
            self.registry.toggle_instance(module_id, false).await?;
        }
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fnn_client::SimulatedFnnClient;
    use crate::module_profile::profile_from_preset;
    use uuid::Uuid;

    #[tokio::test]
    async fn hot_reloader_mounts_lume_yielding_from_storage() {
        let db = Arc::new(
            AgentDb::open_path(std::env::temp_dir().join(format!(
                "fa-hot-reload-{}.db",
                Uuid::new_v4()
            )))
            .expect("open db"),
        );
        db.install_module("lume_yielding", true, "{}")
            .expect("install");

        let ctx = Arc::new(SidecarBootContext {
            agent_id: 1,
            fnn_client: Arc::new(SimulatedFnnClient::new(1)),
            db: db.clone(),
            member_id: Uuid::new_v4(),
            profile: profile_from_preset(
                1,
                crate::module_profile::SidecarProfilePreset::Full,
                "test",
            ),
        });
        let (tx, _rx) = mpsc::channel(4);
        let registry = DynamicModuleRegistry::new(tx);
        let reloader = HotReloader::new(registry, db, ctx);
        reloader.hydrate_from_storage().await.expect("hydrate");
        assert!(reloader.registry.is_mounted("lume_yielding").await);
    }

    #[tokio::test]
    async fn hydrate_does_not_reseed_after_user_uninstalls_all() {
        let db = Arc::new(
            AgentDb::open_path(std::env::temp_dir().join(format!(
                "fa-hot-reload-clear-{}.db",
                Uuid::new_v4()
            )))
            .expect("open db"),
        );
        db.install_module("lume_yielding", true, "{}")
            .expect("install");
        db.mark_module_registry_bootstrapped()
            .expect("bootstrap");

        let ctx = Arc::new(SidecarBootContext {
            agent_id: 1,
            fnn_client: Arc::new(SimulatedFnnClient::new(1)),
            db: db.clone(),
            member_id: Uuid::new_v4(),
            profile: profile_from_preset(
                1,
                crate::module_profile::SidecarProfilePreset::Full,
                "test",
            ),
        });
        let (tx, _rx) = mpsc::channel(4);
        let registry = DynamicModuleRegistry::new(tx);
        let reloader = HotReloader::new(registry, db.clone(), ctx);
        reloader
            .uninstall("lume_yielding")
            .await
            .expect("uninstall");
        reloader.hydrate_from_storage().await.expect("hydrate");
        assert!(!reloader.registry.is_mounted("lume_yielding").await);
        assert!(db.get_installed_modules().expect("installed").is_empty());
    }
}
