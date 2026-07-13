//! Shared clearing types, hub liquidity pools, and fiat→Shannon conversion.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use dashmap::DashMap;
use mesh_core::error::MeshError;
use mesh_core::types::L2Asset;
use serde::{Deserialize, Serialize};
use fsp_fixed_math::round_fiat_to_shannons;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Open enterprise clearing parameters — loaded from `clearing-policy.yml` / JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentSwapParameters {
    /// Maximum single intent-swap volume in Shannon atoms before escalation.
    pub max_volume_shannons: u64,
    /// FX spread applied to cross-hub swaps, in basis points.
    pub fx_spread_bps: u32,
    /// When true, fiat collateral must be confirmed before hub routing proceeds.
    pub require_fiat_collateral: bool,
    /// Hub display names permitted for regional clearing routes.
    pub hub_routing_whitelist: Vec<String>,
    /// Maximum per-leg volume for multi-asset RWA / xUDT cross-clearing.
    pub max_multi_asset_volume: u64,
}

/// Macro RWA cross-clearing intent — swap RGB++ RWA for xUDT stablecoin (or vice versa).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiAssetCrossClearingIntent {
    pub agent_id: u16,
    pub destination_agent: u16,
    pub source_asset: L2Asset,
    pub target_asset: L2Asset,
    pub source_amount: u64,
    pub min_target_amount: u64,
}

#[derive(Debug, Clone)]
pub struct MatchedSwapLeg {
    pub asset: L2Asset,
    pub amount: u64,
    pub path: Vec<u16>,
    pub swap_id: Uuid,
}

impl Default for IntentSwapParameters {
    fn default() -> Self {
        Self {
            max_volume_shannons: 500_000_000,
            fx_spread_bps: 25,
            require_fiat_collateral: true,
            hub_routing_whitelist: vec![
                "Agent-Localized-RUSD-Vault".to_string(),
                "Corporate-Clearing-Vault".to_string(),
            ],
            max_multi_asset_volume: 250_000_000,
        }
    }
}

impl IntentSwapParameters {
    pub fn load_from_file(path: &str) -> Self {
        let path = Path::new(path);
        if !path.is_file() {
            log::warn!(
                "Clearing policy file missing ({}); using IntentSwapParameters::default()",
                path.display()
            );
            return Self::default();
        }

        let raw = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                log::warn!(
                    "Clearing policy read failed ({}): {err}; using defaults",
                    path.display()
                );
                return Self::default();
            }
        };

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let parsed: Result<IntentSwapParameters, String> = if extension == "json" {
            serde_json::from_str(&raw).map_err(|err| err.to_string())
        } else {
            serde_yaml::from_str(&raw).map_err(|err| err.to_string())
        };

        match parsed {
            Ok(params) => params,
            Err(err) => {
                log::warn!(
                    "Clearing policy parse failed ({}): {err}; using defaults",
                    path.display()
                );
                Self::default()
            }
        }
    }
}

/// Per-hub L1 liquidity tracked for concurrent intent-swap reservation.
#[derive(Debug, Clone, Copy)]
pub struct HubAssetState {
    pub available: u64,
}

/// Regional clearing engine — DashMap for lock-free hub lookup, per-hub `Mutex` for atomic debits.
pub struct RegionalClearinghouseEngine {
    pub hub_liquidity_pools: DashMap<String, Arc<Mutex<HubAssetState>>>,
}

impl Default for RegionalClearinghouseEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RegionalClearinghouseEngine {
    pub fn new() -> Self {
        Self {
            hub_liquidity_pools: DashMap::new(),
        }
    }

    /// Registers a hub pool if missing (does not overwrite an existing tracked balance).
    pub fn bootstrap_pool(&self, hub_id: impl Into<String>, initial_available: u64) {
        let hub_id = hub_id.into();
        self.hub_liquidity_pools
            .entry(hub_id)
            .or_insert_with(|| Arc::new(Mutex::new(HubAssetState {
                available: initial_available,
            })));
    }

    pub async fn available_capacity(&self, hub_id: &str) -> Result<u64, MeshError> {
        let pool = self
            .hub_liquidity_pools
            .get(hub_id)
            .ok_or_else(|| MeshError::HubNotFound(hub_id.to_string()))?;
        let available = pool.lock().await.available;
        Ok(available)
    }

    /// Reserves outbound hub liquidity for an in-flight intent swap (prevents double-spend races).
    pub async fn execute_intent_swap(
        &self,
        hub_id: &str,
        required_capacity: u64,
    ) -> Result<(), MeshError> {
        let pool = self
            .hub_liquidity_pools
            .get(hub_id)
            .ok_or_else(|| MeshError::HubNotFound(hub_id.to_string()))?;

        let mut state = pool.lock().await;
        if state.available >= required_capacity {
            state.available -= required_capacity;
            Ok(())
        } else {
            Err(MeshError::InsufficientFloat)
        }
    }

    /// Returns reserved liquidity when a swap aborts before settlement.
    pub async fn credit_capacity(&self, hub_id: &str, amount: u64) -> Result<(), MeshError> {
        let pool = self
            .hub_liquidity_pools
            .get(hub_id)
            .ok_or_else(|| MeshError::HubNotFound(hub_id.to_string()))?;
        let mut state = pool.lock().await;
        state.available = state.available.saturating_add(amount);
        Ok(())
    }
}

pub fn hub_pool_key(hub_id: Uuid) -> String {
    hub_id.to_string()
}

pub async fn convert_fiat_to_shannons(
    registry: &mesh_core::currency::AssetRegistryHub,
    source_iso: &str,
    fiat_amount: f64,
) -> Result<u64, String> {
    let assets = registry.assets.read().await;
    let config = assets.get(source_iso).ok_or_else(|| {
        format!("Currency configuration missing for corridor ISO '{source_iso}'")
    })?;
    Ok(round_fiat_to_shannons(
        fiat_amount,
        config.atomic_decimals,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::currency::{AssetRegistryHub, CurrencyAssetConfig};

    #[tokio::test]
    async fn convert_fiat_to_shannons_uses_registry_atomic_scale() {
        let registry = AssetRegistryHub::new();
        registry
            .introduce_currency_asset(CurrencyAssetConfig {
                iso_code: "TZS".to_string(),
                country_name: "Tanzania".to_string(),
                atomic_decimals: 8,
                udt_code_hash: "0xabc".to_string(),
                udt_args: "0x01".to_string(),
                macro_velocity_limit_24h: 100_000_000.0,
            })
            .await;

        let shannons = convert_fiat_to_shannons(&registry, "TZS", 1_000.0)
            .await
            .expect("conversion");

        assert_eq!(shannons, 100_000_000_000);
    }

    #[tokio::test]
    async fn execute_intent_swap_rejects_double_spend_race() {
        let engine = Arc::new(RegionalClearinghouseEngine::new());
        engine.bootstrap_pool("hub-a", 50_000_000);

        let first = engine.execute_intent_swap("hub-a", 30_000_000);
        let second = engine.execute_intent_swap("hub-a", 30_000_000);

        assert!(first.await.is_ok());
        assert_eq!(second.await, Err(MeshError::InsufficientFloat));
        assert_eq!(engine.available_capacity("hub-a").await.unwrap(), 20_000_000);
    }

    #[tokio::test]
    async fn credit_capacity_restores_aborted_reservation() {
        let engine = RegionalClearinghouseEngine::new();
        engine.bootstrap_pool("hub-b", 10_000_000);
        engine
            .execute_intent_swap("hub-b", 4_000_000)
            .await
            .expect("reserve");
        engine
            .credit_capacity("hub-b", 4_000_000)
            .await
            .expect("rollback");
        assert_eq!(engine.available_capacity("hub-b").await.unwrap(), 10_000_000);
    }
}
