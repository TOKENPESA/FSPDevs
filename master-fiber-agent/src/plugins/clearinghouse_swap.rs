//! Regional clearinghouse swap plugin — telco B2C float refill + cross-hub intent settlement.

use std::sync::Arc;

use async_trait::async_trait;
use mesh_core::compliance::IntentSwapOrder as ComplianceIntentSwapOrder;
use mesh_core::papss_types::PapssSettlementStatus;
use mesh_core::types::{FiatProvider, FloatExhaustionTelemetry, L2Asset};
use fsp_fixed_math::apply_bps_spread_shannons;
use uuid::Uuid;

use crate::clearing::{
    convert_fiat_to_shannons, hub_pool_key, IntentSwapParameters, MatchedSwapLeg,
    MultiAssetCrossClearingIntent,
};
use crate::config::{
    fiat_provider_corridor_iso, papss_macro_rebalance_threshold_fiat,
    sovereign_levy_rate, telco_clearing_mock_when_unset,
};
use crate::hub::{execute_cross_hub_intent_swap, settle_cross_hub_intent_swap, HubAccount};
use crate::papss::PapssIntegrationGateway;
use crate::routing::{find_asset_aware_route, RouteQuery};
use crate::state::AppState;
use crate::traits::MfaClearingPlugin;

pub struct ClearinghouseSwapModule {
    telco_api_endpoint: String,
    swap_parameters: IntentSwapParameters,
}

impl ClearinghouseSwapModule {
    pub fn new(telco_api_endpoint: String) -> Self {
        let policy_path = std::env::var("MFA_CLEARING_POLICY_PATH")
            .unwrap_or_else(|_| "clearing-policy.yml".to_string());
        Self {
            telco_api_endpoint,
            swap_parameters: IntentSwapParameters::load_from_file(&policy_path),
        }
    }

    pub fn swap_parameters(&self) -> &IntentSwapParameters {
        &self.swap_parameters
    }

    async fn resolve_clearing_hub_pair_for_asset(
        &self,
        state: &AppState,
        asset: &L2Asset,
    ) -> Result<(Uuid, Uuid), String> {
        let label = asset.ledger_label();
        let whitelist = &self.swap_parameters.hub_routing_whitelist;
        let mut registry = state.multi_hub_registry.write().await;
        let hub_ids: Vec<Uuid> = registry
            .hubs
            .values()
            .filter(|hub| hub.supported_assets.iter().any(|a| a == &label))
            .filter(|hub| {
                whitelist.is_empty() || whitelist.iter().any(|allowed| allowed == &hub.name)
            })
            .map(|hub| hub.hub_id)
            .collect();

        if hub_ids.len() >= 2 {
            return Ok((hub_ids[0], hub_ids[1]));
        }

        let rpc_url = state.hub_config.rpc_url.clone();
        let source_hub_id = Uuid::new_v4();
        let target_hub_id = Uuid::new_v4();
        let supported = vec!["CKB".to_string(), "RUSD".to_string(), label.clone()];

        registry.hubs.insert(
            source_hub_id,
            HubAccount {
                hub_id: source_hub_id,
                name: format!("Agent-Localized-{label}-Vault"),
                rpc_url: rpc_url.clone(),
                public_key_hex: format!("03agent-{label}-vault"),
                supported_assets: supported.clone(),
                available_l1_balance_shannons: 500_000_000,
            },
        );
        registry.hubs.insert(
            target_hub_id,
            HubAccount {
                hub_id: target_hub_id,
                name: format!("Corporate-Clearing-{label}-Vault"),
                rpc_url,
                public_key_hex: format!("03corporate-{label}-vault"),
                supported_assets: supported,
                available_l1_balance_shannons: 1_000_000_000,
            },
        );

        state
            .regional_clearing
            .bootstrap_pool(hub_pool_key(source_hub_id), 500_000_000);
        state
            .regional_clearing
            .bootstrap_pool(hub_pool_key(target_hub_id), 1_000_000_000);

        Ok((source_hub_id, target_hub_id))
    }

    async fn resolve_clearing_hub_pair(&self, state: &AppState) -> Result<(Uuid, Uuid), String> {
        self.resolve_clearing_hub_pair_for_asset(state, &L2Asset::RusdStablecoin)
            .await
    }

    async fn trigger_papss_macro_rebalance(
        &self,
        papss_gateway: &PapssIntegrationGateway,
        imbalance_fiat_amount: f64,
        source_iso: &str,
    ) -> Result<(), String> {
        let reference_id = Uuid::new_v4()
            .to_string()
            .replace('-', "")[..12]
            .to_string();

        log::warn!(
            "⚖️ [CLEARINGHOUSE] Massive liquidity divergence detected. Triggering PAPSS Macro-Settlement."
        );

        let receipt = papss_gateway
            .execute_macro_rebalance(
                imbalance_fiat_amount,
                source_iso,
                "CRDBTZTZ",
                "EQBKKENX",
                &reference_id,
            )
            .await?;

        if let PapssSettlementStatus::Settled = receipt.status {
            log::info!(
                "🔗 [L2 SYNC] PAPSS settlement confirmed. Realigning CKB/RUSD channel capacities."
            );
        }

        Ok(())
    }

    async fn trigger_carrier_b2c_payout(&self, provider: &FiatProvider, amount_fiat: f64) -> bool {
        if self.telco_api_endpoint.is_empty() {
            if telco_clearing_mock_when_unset() {
                log::warn!(
                    "⚠️ [CLEARING] MFA_TELCO_CLEARING_API_URL unset — mock telco payout accepted ({provider:?}, {amount_fiat})"
                );
                return true;
            }
            eprintln!(
                "⚠️ [CLEARING] Telco API endpoint unset — skipping carrier B2C dispatch ({provider:?}). Set MFA_TELCO_CLEARING_API_URL or MFA_TELCO_CLEARING_MOCK=1."
            );
            return false;
        }

        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "CommandID": "BusinessPayment",
            "Amount": amount_fiat,
            "Remarks": "Automated Fiber-Sidecar Float Liquidation Pipeline"
        });

        match client
            .post(&self.telco_api_endpoint)
            .json(&payload)
            .send()
            .await
        {
            Ok(res) => res.status().is_success(),
            Err(err) => {
                eprintln!("⚠️ [CLEARING] Carrier gateway error: {err}");
                false
            }
        }
    }
}

#[async_trait]
impl MfaClearingPlugin for ClearinghouseSwapModule {
    fn plugin_name(&self) -> &'static str {
        "clearinghouse_swap"
    }

    async fn handle_float_crisis(
        &self,
        state: Arc<AppState>,
        telemetry: FloatExhaustionTelemetry,
    ) -> Result<(), String> {
        println!(
            "🔄 [CLEARING PLUGIN] Processing rebalance event for Agent Node FA-{}",
            telemetry.agent_id
        );

        let rebalance_target_fiat =
            (telemetry.critical_fiat_floor - telemetry.current_fiat_balance) + 250_000.0;

        let (source_iso, target_iso) = fiat_provider_corridor_iso(&telemetry.provider);

        let currency_config = {
            let assets = state.asset_registry.assets.read().await;
            assets.get(source_iso).cloned().ok_or_else(|| {
                format!("Currency configuration missing for corridor ISO '{source_iso}'")
            })?
        };

        let compliance_swap = ComplianceIntentSwapOrder {
            swap_id: Uuid::new_v4(),
            infrastructure_channel_id: Uuid::new_v4(),
            counterparty_pubkey: format!("FA-{}", telemetry.agent_id),
            target_asset_symbol: target_iso.to_string(),
            expiration_locktime: chrono::Utc::now().timestamp() as u64 + 3600,
        };

        let estimated_tax_levy = rebalance_target_fiat * sovereign_levy_rate();
        state
            .sovereign_compliance_hub()
            .process_and_enforce_compliance(
                telemetry.agent_id,
                "FLOAT_CRISIS_REBALANCE",
                source_iso,
                target_iso,
                rebalance_target_fiat,
                estimated_tax_levy,
                &currency_config,
                &compliance_swap,
            )
            .await?;

        let mut equivalent_shannons =
            convert_fiat_to_shannons(&state.asset_registry, source_iso, rebalance_target_fiat)
                .await?;

        equivalent_shannons = apply_bps_spread_shannons(
            equivalent_shannons,
            self.swap_parameters.fx_spread_bps,
        );

        if equivalent_shannons > self.swap_parameters.max_volume_shannons {
            return Err(format!(
                "Intent swap volume {equivalent_shannons} exceeds configured max_volume_shannons {}",
                self.swap_parameters.max_volume_shannons
            ));
        }

        if self.swap_parameters.require_fiat_collateral && telemetry.current_fiat_balance <= 0.0 {
            return Err(
                "Fiat collateral required by clearing policy but agent float is depleted".to_string(),
            );
        }

        let (source_hub_id, target_hub_id) = self.resolve_clearing_hub_pair(&state).await?;

        let swap_nonce = Uuid::new_v4();
        let payment_hash = format!("hash-rebalance-{swap_nonce}-fsp");
        let preimage = format!("pre-rebalance-{swap_nonce}-fsp");

        let swap_id = execute_cross_hub_intent_swap(
            state.clone(),
            source_hub_id,
            target_hub_id,
            "RUSD".to_string(),
            equivalent_shannons,
            payment_hash,
        )
        .await
        .map_err(|e| format!("Clearinghouse multi-hub asset lock failed: {e}"))?;

        if rebalance_target_fiat > papss_macro_rebalance_threshold_fiat() {
            if let Some(ref papss_gateway) = state.papss_gateway {
                self.trigger_papss_macro_rebalance(
                    papss_gateway,
                    rebalance_target_fiat,
                    source_iso,
                )
                .await?;
            }
        }

        let dispatch_success = self
            .trigger_carrier_b2c_payout(&telemetry.provider, rebalance_target_fiat)
            .await;

        if dispatch_success {
            settle_cross_hub_intent_swap(state.clone(), swap_id, preimage)
                .await
                .map_err(|e| {
                    format!("Critical Fault: Failed to settle off-chain balancing metrics: {e}")
                })?;

            println!(
                "✅ [CLEARING COMPLETE] Account balance refilled via API payout. L2 token pools rebalanced."
            );
            Ok(())
        } else {
            let _ = state
                .regional_clearing
                .credit_capacity(&hub_pool_key(target_hub_id), equivalent_shannons)
                .await;
            if let Some(tgt_hub) = state.multi_hub_registry.write().await.hubs.get_mut(&target_hub_id)
            {
                tgt_hub.available_l1_balance_shannons = state
                    .regional_clearing
                    .available_capacity(&hub_pool_key(target_hub_id))
                    .await
                    .unwrap_or(tgt_hub.available_l1_balance_shannons.saturating_add(equivalent_shannons));
            }
            Err("Telco infrastructure gateway timeout. Rebalancing circuit aborted.".to_string())
        }
    }

    async fn handle_multi_asset_cross_clearing(
        &self,
        state: Arc<AppState>,
        intent: MultiAssetCrossClearingIntent,
    ) -> Result<Vec<MatchedSwapLeg>, String> {
        if intent.source_amount == 0 || intent.min_target_amount == 0 {
            return Err("source_amount and min_target_amount must be non-zero".to_string());
        }
        if intent.agent_id == intent.destination_agent {
            return Err("agent_id and destination_agent must differ".to_string());
        }
        if intent.source_asset == intent.target_asset {
            return Err("cross-clearing requires distinct source and target assets".to_string());
        }

        if intent.source_amount > self.swap_parameters.max_multi_asset_volume
            || intent.min_target_amount > self.swap_parameters.max_multi_asset_volume
        {
            return Err(format!(
                "Multi-asset leg exceeds max_multi_asset_volume {}",
                self.swap_parameters.max_multi_asset_volume
            ));
        }

        let edge_limit = state
            .simulation_edge_nodes
            .load(std::sync::atomic::Ordering::Relaxed);

        let (source_path, target_path) = {
            let graph = state.graph.read().await;
            let outbound = find_asset_aware_route(
                &graph,
                RouteQuery {
                    source: intent.agent_id,
                    destination: intent.destination_agent,
                    required_amount: intent.source_amount,
                    target_asset: intent.source_asset.clone(),
                    network_limit: Some(edge_limit),
                },
                Some(&state.plugin_registry),
            )
            .ok_or_else(|| {
                format!(
                    "No RGB++/xUDT path for {:?} from FA-{} → FA-{}",
                    intent.source_asset, intent.agent_id, intent.destination_agent
                )
            })?;

            let inbound = find_asset_aware_route(
                &graph,
                RouteQuery {
                    source: intent.destination_agent,
                    destination: intent.agent_id,
                    required_amount: intent.min_target_amount,
                    target_asset: intent.target_asset.clone(),
                    network_limit: Some(edge_limit),
                },
                Some(&state.plugin_registry),
            )
            .ok_or_else(|| {
                format!(
                    "No return path for {:?} from FA-{} → FA-{}",
                    intent.target_asset, intent.destination_agent, intent.agent_id
                )
            })?;

            (outbound.0, inbound.0)
        };

        let source_label = intent.source_asset.ledger_label();
        let target_label = intent.target_asset.ledger_label();

        let (source_hubs, target_hubs) = tokio::join!(
            self.resolve_clearing_hub_pair_for_asset(&state, &intent.source_asset),
            self.resolve_clearing_hub_pair_for_asset(&state, &intent.target_asset),
        );
        let (source_hub_id, source_hub_out) = source_hubs?;
        let (target_hub_id, target_hub_out) = target_hubs?;

        let source_nonce = Uuid::new_v4();
        let target_nonce = Uuid::new_v4();
        let source_payment_hash = format!("hash-multi-src-{source_nonce}-fsp");
        let target_payment_hash = format!("hash-multi-tgt-{target_nonce}-fsp");

        let (source_swap, target_swap) = tokio::join!(
            execute_cross_hub_intent_swap(
                state.clone(),
                source_hub_id,
                source_hub_out,
                source_label.clone(),
                intent.source_amount,
                source_payment_hash,
            ),
            execute_cross_hub_intent_swap(
                state.clone(),
                target_hub_id,
                target_hub_out,
                target_label.clone(),
                intent.min_target_amount,
                target_payment_hash,
            ),
        );

        let source_swap_id = source_swap
            .map_err(|e| format!("Source asset hub lock failed ({source_label}): {e}"))?;
        let target_swap_id = target_swap
            .map_err(|e| format!("Target asset hub lock failed ({target_label}): {e}"))?;

        let source_preimage = format!("pre-multi-src-{source_nonce}-fsp");
        let target_preimage = format!("pre-multi-tgt-{target_nonce}-fsp");

        let (source_settle, target_settle) = tokio::join!(
            settle_cross_hub_intent_swap(state.clone(), source_swap_id, source_preimage),
            settle_cross_hub_intent_swap(state.clone(), target_swap_id, target_preimage),
        );
        source_settle.map_err(|e| format!("Source leg settlement failed: {e}"))?;
        target_settle.map_err(|e| format!("Target leg settlement failed: {e}"))?;

        log::info!(
            "✅ [MULTI-ASSET CLEARING] FA-{} swapped {} {} for ≥{} {} across enterprise hubs",
            intent.agent_id,
            intent.source_amount,
            source_label,
            intent.min_target_amount,
            target_label,
        );

        Ok(vec![
            MatchedSwapLeg {
                asset: intent.source_asset,
                amount: intent.source_amount,
                path: source_path,
                swap_id: source_swap_id,
            },
            MatchedSwapLeg {
                asset: intent.target_asset,
                amount: intent.min_target_amount,
                path: target_path,
                swap_id: target_swap_id,
            },
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::currency::AssetRegistryHub;
    use mesh_core::types::L2Asset;

    use crate::graph::{AssetCapacityMap, LiveMeshEdge};
    use crate::plugin_registry::PluginRegistry;
    use crate::test_support;

    #[tokio::test]
    async fn multi_asset_cross_clearing_settles_rgbpp_for_xudt() {
        let registry = AssetRegistryHub::new();
        let state = test_support::app_state_with_registry(
            tokio::sync::broadcast::channel(4).0,
            registry,
        );

        let rgb_hash = "0xstock".to_string();
        let udt_hash = "0xstable".to_string();
        let mut caps_out = AssetCapacityMap::new();
        caps_out.insert(L2Asset::CkbNative, 10_000_000);
        caps_out.insert(L2Asset::RgbPlusPlus(rgb_hash.clone()), 5_000_000);
        let mut caps_in = AssetCapacityMap::new();
        caps_in.insert(L2Asset::CkbNative, 10_000_000);
        caps_in.insert(L2Asset::UDT(udt_hash.clone()), 8_000_000);

        {
            let mut graph = state.graph.write().await;
            graph.adjacency_map.insert(
                1,
                vec![LiveMeshEdge {
                    channel_id: "c-1-2".into(),
                    peer_id: 2,
                    peer_pubkey: String::new(),
                    capacity_shannons: 10_000_000,
                    asset_capacities: caps_out,
                    fee_base: 0,
                    fee_proportional: 0,
                    is_active: true,
                    last_update_timestamp: 0,
                }],
            );
            graph.adjacency_map.insert(
                2,
                vec![LiveMeshEdge {
                    channel_id: "c-2-1".into(),
                    peer_id: 1,
                    peer_pubkey: String::new(),
                    capacity_shannons: 10_000_000,
                    asset_capacities: caps_in,
                    fee_base: 0,
                    fee_proportional: 0,
                    is_active: true,
                    last_update_timestamp: 0,
                }],
            );
        }

        state
            .simulation_edge_nodes
            .store(16, std::sync::atomic::Ordering::Relaxed);

        let plugin = ClearinghouseSwapModule::new(String::new());
        let legs = plugin
            .handle_multi_asset_cross_clearing(
                state,
                MultiAssetCrossClearingIntent {
                    agent_id: 1,
                    destination_agent: 2,
                    source_asset: L2Asset::RgbPlusPlus(rgb_hash),
                    target_asset: L2Asset::UDT(udt_hash),
                    source_amount: 1_000_000,
                    min_target_amount: 900_000,
                },
            )
            .await
            .expect("multi-asset clearing");

        assert_eq!(legs.len(), 2);
        assert_eq!(legs[0].path, vec![1, 2]);
        assert_eq!(legs[1].path, vec![2, 1]);
    }

    #[test]
    fn plugin_registry_includes_clearinghouse_swap() {
        let registry = PluginRegistry::bootstrap_default(1_000_000);
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        assert_eq!(
            rt.block_on(registry.clearing_plugin_name()),
            Some("clearinghouse_swap")
        );
    }
}
