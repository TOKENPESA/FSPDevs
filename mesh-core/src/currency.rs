use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CurrencyAssetConfig {
    /// ISO currency code (e.g., "TZS", "KES", "NGN", "ZAR", "USD").
    pub iso_code: String,
    pub country_name: String,
    /// Atomic decimal scale (e.g., 8 for CKB UDT cell alignments).
    pub atomic_decimals: u32,
    /// Layer 1 cell definition lock script identifier.
    pub udt_code_hash: String,
    /// Isomorphic token identifier binding script arguments.
    pub udt_args: String,
    /// Sovereign threshold cap protecting against capital flight (24h velocity).
    pub macro_velocity_limit_24h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpotMarketRate {
    pub pair_id: Uuid,
    /// Base currency (e.g., "TZS").
    pub base_currency: String,
    /// Quote currency (e.g., "KES").
    pub quote_currency: String,
    /// Direct price ratio conversion scalar.
    pub exchange_rate: f64,
    /// Unix milestone timestamp.
    pub last_oracle_update: u64,
    /// Central-bank mandated transaction corridor spread markup.
    pub regulatory_spread_markup: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DynamicAssetRegistry {
    pub asset_iso: String,
    /// Absolute currency value scaled up to prevent rounding differences.
    /// Calculated using a base multiplier factor of 10^8 (integer atomic units).
    pub scaled_exchange_rate: u64,
}

impl DynamicAssetRegistry {
    pub fn convert_fiat_to_shannons(&self, fiat_amount_units: u64) -> u64 {
        // Enforce deterministic pure integer operations across all execution nodes
        fiat_amount_units.saturating_mul(self.scaled_exchange_rate)
    }
}

fn atomic_scale_multiplier(atomic_decimals: u32) -> u64 {
    fsp_fixed_math::atomic_scale_multiplier(atomic_decimals)
}

impl From<&CurrencyAssetConfig> for DynamicAssetRegistry {
    fn from(config: &CurrencyAssetConfig) -> Self {
        Self {
            asset_iso: config.iso_code.clone(),
            scaled_exchange_rate: atomic_scale_multiplier(config.atomic_decimals),
        }
    }
}

#[derive(Clone, Default)]
pub struct AssetRegistryHub {
    pub assets: Arc<RwLock<HashMap<String, CurrencyAssetConfig>>>,
    pub rate_oracle: Arc<RwLock<HashMap<String, SpotMarketRate>>>,
}

impl std::fmt::Debug for AssetRegistryHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetRegistryHub")
            .field("assets", &self.assets)
            .field("rate_oracle", &self.rate_oracle)
            .finish_non_exhaustive()
    }
}

impl AssetRegistryHub {
    pub fn new() -> Self {
        Self {
            assets: Arc::new(RwLock::new(HashMap::new())),
            rate_oracle: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Dynamically injects a newly approved cross-border asset script structure into active routing tables.
    pub async fn introduce_currency_asset(&self, config: CurrencyAssetConfig) {
        let mut asset_guard = self.assets.write().await;
        log::info!(
            "[REGISTRY] Dynamic asset corridor added: {} ({})",
            config.iso_code,
            config.country_name
        );
        asset_guard.insert(config.iso_code.clone(), config);
    }

    /// Feeds real-time exchange rates into the active L2 settlement channels.
    pub async fn apply_spot_market_rate(&self, rate: SpotMarketRate) {
        let mut rate_guard = self.rate_oracle.write().await;
        let corridor_key = format!("{}-{}", rate.base_currency, rate.quote_currency);
        log::info!(
            "[ORACLE] Direct exchange rate update for {corridor_key}: {}",
            rate.exchange_rate
        );
        rate_guard.insert(corridor_key, rate);
    }

    /// Evaluates currency path logic to transform fiat values into atomic token units.
    pub async fn compute_conversion(
        &self,
        source_iso: &str,
        target_iso: &str,
        base_amount: f64,
    ) -> Result<(f64, u64), String> {
        let assets = self.assets.read().await;
        let rates = self.rate_oracle.read().await;

        let target_asset = assets.get(target_iso).ok_or_else(|| {
            format!(
                "Target asset configuration code '{target_iso}' not recognized by protocol maps"
            )
        })?;

        let transformation_ratio = if source_iso == target_iso {
            1.0
        } else {
            let corridor_key = format!("{source_iso}-{target_iso}");
            rates
                .get(&corridor_key)
                .map(|r| r.exchange_rate)
                .ok_or_else(|| {
                    format!("No active oracle spread matching found for corridor: {corridor_key}")
                })?
        };

        let converted_fiat_value = base_amount * transformation_ratio;
        let dynamic_asset = DynamicAssetRegistry::from(target_asset);
        let fiat_units = converted_fiat_value.max(0.0) as u64;
        let atomic_token_units = dynamic_asset.convert_fiat_to_shannons(fiat_units);

        Ok((converted_fiat_value, atomic_token_units))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tzs_asset() -> CurrencyAssetConfig {
        CurrencyAssetConfig {
            iso_code: "TZS".to_string(),
            country_name: "Tanzania".to_string(),
            atomic_decimals: 8,
            udt_code_hash: "0xabc".to_string(),
            udt_args: "0x01".to_string(),
            macro_velocity_limit_24h: 1_000_000.0,
        }
    }

    fn kes_asset() -> CurrencyAssetConfig {
        CurrencyAssetConfig {
            iso_code: "KES".to_string(),
            country_name: "Kenya".to_string(),
            atomic_decimals: 8,
            udt_code_hash: "0xdef".to_string(),
            udt_args: "0x02".to_string(),
            macro_velocity_limit_24h: 800_000.0,
        }
    }

    #[test]
    fn dynamic_asset_registry_convert_fiat_to_shannons_uses_integer_scale() {
        let registry = DynamicAssetRegistry {
            asset_iso: "TZS".to_string(),
            scaled_exchange_rate: 100_000_000,
        };
        assert_eq!(registry.convert_fiat_to_shannons(1_000), 100_000_000_000);
    }

    #[tokio::test]
    async fn compute_conversion_same_currency_is_identity() {
        let registry = AssetRegistryHub::new();
        registry.introduce_currency_asset(tzs_asset()).await;

        let (fiat, atomic) = registry
            .compute_conversion("TZS", "TZS", 1_000.0)
            .await
            .expect("conversion");

        assert_eq!(fiat, 1_000.0);
        assert_eq!(atomic, 100_000_000_000);
    }

    #[tokio::test]
    async fn compute_conversion_uses_oracle_corridor() {
        let registry = AssetRegistryHub::new();
        registry.introduce_currency_asset(tzs_asset()).await;
        registry.introduce_currency_asset(kes_asset()).await;
        registry
            .apply_spot_market_rate(SpotMarketRate {
                pair_id: Uuid::new_v4(),
                base_currency: "TZS".to_string(),
                quote_currency: "KES".to_string(),
                exchange_rate: 0.05,
                last_oracle_update: 1_700_000_000,
                regulatory_spread_markup: 0.001,
            })
            .await;

        let (fiat, atomic) = registry
            .compute_conversion("TZS", "KES", 10_000.0)
            .await
            .expect("conversion");

        assert_eq!(fiat, 500.0);
        assert_eq!(atomic, 50_000_000_000);
    }
}
