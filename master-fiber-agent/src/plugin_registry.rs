//! Plugin registry — hot-swappable policy + clearing plugin matrix.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use mesh_core::types::{FloatExhaustionTelemetry, L2Asset};
use serde_json::Value;

use crate::clearing::{MatchedSwapLeg, MultiAssetCrossClearingIntent};
use crate::config::telco_clearing_api_url;
use crate::plugins::{
    automated_refueling::AutomatedRefuelingBrain, clearinghouse_swap::ClearinghouseSwapModule,
    lume_pricing::LumePricingEngine, sovereign_compliance::SovereignComplianceFilter,
};
use crate::state::AppState;
use crate::traits::{ClearanceVerdict, MfaClearingPlugin, MfaPolicyPlugin, PolicyError, RoutingIntent};

struct PolicySlot {
    plugin: Arc<dyn MfaPolicyPlugin>,
    active: bool,
}

struct PluginRegistryInner {
    policy_plugins: RwLock<HashMap<String, PolicySlot>>,
    clearing_plugin: RwLock<Option<Arc<dyn MfaClearingPlugin>>>,
}

/// Thread-safe plugin matrix (`Send + Sync`, cloneable via inner `Arc`).
#[derive(Clone)]
pub struct PluginRegistry {
    inner: Arc<PluginRegistryInner>,
}

impl PluginRegistry {
    pub fn new(
        policy_plugins: Vec<Arc<dyn MfaPolicyPlugin>>,
        clearing_plugin: Arc<dyn MfaClearingPlugin>,
    ) -> Self {
        let mut map = HashMap::new();
        for plugin in policy_plugins {
            let name = plugin.plugin_name().to_string();
            map.insert(
                name,
                PolicySlot {
                    plugin,
                    active: true,
                },
            );
        }
        Self {
            inner: Arc::new(PluginRegistryInner {
                policy_plugins: RwLock::new(map),
                clearing_plugin: RwLock::new(Some(clearing_plugin)),
            }),
        }
    }

    /// Boot-time registration of standard MFA policy modules + clearinghouse swap plugin.
    pub fn bootstrap_default(critical_capacity_floor: u64) -> Self {
        Self::new(
            vec![
                Arc::new(LumePricingEngine::new()),
                Arc::new(SovereignComplianceFilter::new()),
                Arc::new(AutomatedRefuelingBrain::new(critical_capacity_floor)),
            ],
            Arc::new(ClearinghouseSwapModule::new(telco_clearing_api_url())),
        )
    }

    /// Empty registry — plugins are mounted exclusively via `PluginHotReloader::hydrate_from_storage`.
    pub fn empty() -> Self {
        Self {
            inner: Arc::new(PluginRegistryInner {
                policy_plugins: RwLock::new(HashMap::new()),
                clearing_plugin: RwLock::new(None),
            }),
        }
    }

    pub async fn mount_policy_plugin(
        &self,
        plugin_id: &str,
        plugin: Arc<dyn MfaPolicyPlugin>,
        active: bool,
    ) {
        let mut guard = self.inner.policy_plugins.write().expect("policy plugin lock");
        guard.insert(
            plugin_id.to_string(),
            PolicySlot { plugin, active },
        );
    }

    pub async fn mount_clearing_plugin(&self, plugin: Arc<dyn MfaClearingPlugin>) {
        let mut guard = self.inner.clearing_plugin.write().expect("clearing plugin lock");
        *guard = Some(plugin);
    }

    pub async fn unmount_plugin(&self, plugin_id: &str) -> bool {
        if plugin_id == "clearinghouse_swap" {
            let mut guard = self.inner.clearing_plugin.write().expect("clearing plugin lock");
            return guard.take().is_some();
        }
        let mut guard = self.inner.policy_plugins.write().expect("policy plugin lock");
        guard.remove(plugin_id).is_some()
    }

    pub async fn set_plugin_active(&self, plugin_id: &str, active: bool) {
        let mut guard = self.inner.policy_plugins.write().expect("policy plugin lock");
        if let Some(slot) = guard.get_mut(plugin_id) {
            slot.active = active;
        }
    }

    pub async fn has_plugin(&self, plugin_id: &str) -> bool {
        if plugin_id == "clearinghouse_swap" {
            return self.inner.clearing_plugin.read().expect("clearing plugin lock").is_some();
        }
        self.inner
            .policy_plugins
            .read()
            .expect("policy plugin lock")
            .contains_key(plugin_id)
    }

    pub async fn plugin_names(&self) -> Vec<String> {
        let guard = self.inner.policy_plugins.read().expect("policy plugin lock");
        let mut names: Vec<String> = guard
            .iter()
            .filter(|(_, slot)| slot.active)
            .map(|(_, slot)| slot.plugin.plugin_name().to_string())
            .collect();
        names.sort_unstable();
        if self
            .inner
            .clearing_plugin
            .read()
            .expect("clearing plugin lock")
            .is_some()
        {
            names.push("clearinghouse_swap".to_string());
        }
        names
    }

    pub fn plugin_names_sync(&self) -> Vec<String> {
        let guard = self.inner.policy_plugins.read().expect("policy plugin lock");
        let mut names: Vec<String> = guard
            .iter()
            .filter(|(_, slot)| slot.active)
            .map(|(id, _)| id.clone())
            .collect();
        names.sort_unstable();
        if self
            .inner
            .clearing_plugin
            .read()
            .expect("clearing plugin lock")
            .is_some()
        {
            names.push("clearinghouse_swap".to_string());
        }
        names
    }

    pub async fn clearing_plugin_name(&self) -> Option<&'static str> {
        if self
            .inner
            .clearing_plugin
            .read()
            .expect("clearing plugin lock")
            .is_some()
        {
            Some("clearinghouse_swap")
        } else {
            None
        }
    }

    /// Chain synchronous edge-weight adjustments (physics loop — brief read lock).
    pub fn adjust_edge_weight(
        &self,
        source: u16,
        target: u16,
        asset: &L2Asset,
        base_weight: u32,
    ) -> u32 {
        let source_id = format!("FA-{source}");
        let target_id = format!("FA-{target}");
        let guard = self.inner.policy_plugins.read().expect("policy plugin lock");
        let mut weight = base_weight;
        for slot in guard.values() {
            if !slot.active {
                continue;
            }
            weight = slot
                .plugin
                .adjust_edge_weight(&source_id, &target_id, asset, weight);
        }
        weight
    }

    pub async fn dispatch_heartbeat(&self, agent_id: &str, payload: &Value) {
        let plugins: Vec<Arc<dyn MfaPolicyPlugin>> = {
            let guard = self.inner.policy_plugins.read().expect("policy plugin lock");
            guard
                .values()
                .filter(|slot| slot.active)
                .map(|slot| slot.plugin.clone())
                .collect()
        };
        for plugin in plugins {
            if let Err(err) = plugin.on_heartbeat(agent_id, payload).await {
                log::warn!(
                    "⚠️ [MFA POLICY] {} heartbeat hook failed for {agent_id}: {err}",
                    plugin.plugin_name()
                );
            }
        }
    }

    /// All active policy plugins must approve; first rejection aborts routing.
    pub async fn pre_route_clearance(
        &self,
        intent: &RoutingIntent,
    ) -> Result<ClearanceVerdict, PolicyError> {
        let plugins: Vec<Arc<dyn MfaPolicyPlugin>> = {
            let guard = self.inner.policy_plugins.read().expect("policy plugin lock");
            guard
                .values()
                .filter(|slot| slot.active)
                .map(|slot| slot.plugin.clone())
                .collect()
        };
        for plugin in plugins {
            match plugin.pre_route_clearance(intent).await {
                Ok(ClearanceVerdict::Approved) => {}
                Ok(ClearanceVerdict::Rejected(reason)) => {
                    return Ok(ClearanceVerdict::Rejected(format!(
                        "{}: {reason}",
                        plugin.plugin_name()
                    )));
                }
                Err(err) => return Err(err),
            }
        }
        Ok(ClearanceVerdict::Approved)
    }

    pub async fn handle_float_crisis(
        &self,
        state: Arc<AppState>,
        telemetry: FloatExhaustionTelemetry,
    ) -> Result<(), String> {
        let plugin = self
            .inner
            .clearing_plugin
            .read()
            .expect("clearing plugin lock")
            .clone();
        let plugin = plugin.ok_or_else(|| "clearing plugin is not mounted".to_string())?;
        plugin.handle_float_crisis(state, telemetry).await
    }

    pub async fn handle_multi_asset_cross_clearing(
        &self,
        state: Arc<AppState>,
        intent: MultiAssetCrossClearingIntent,
    ) -> Result<Vec<MatchedSwapLeg>, String> {
        let plugin = self
            .inner
            .clearing_plugin
            .read()
            .expect("clearing plugin lock")
            .clone();
        let plugin = plugin.ok_or_else(|| "clearing plugin is not mounted".to_string())?;
        plugin
            .handle_multi_asset_cross_clearing(state, intent)
            .await
    }

    pub async fn run_multi_asset_match_loop(
        &self,
        state: Arc<AppState>,
        intents: Vec<MultiAssetCrossClearingIntent>,
    ) -> Vec<Result<MatchedSwapLeg, String>> {
        let plugin = self
            .inner
            .clearing_plugin
            .read()
            .expect("clearing plugin lock")
            .clone();
        match plugin {
            Some(plugin) => plugin.run_multi_asset_match_loop(state, intents).await,
            None => intents
                .into_iter()
                .map(|_| Err("clearing plugin is not mounted".to_string()))
                .collect(),
        }
    }
}
