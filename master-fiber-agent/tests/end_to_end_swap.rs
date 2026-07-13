use tokio::sync::broadcast;

use master_fiber_agent::test_support::app_state_with_registry;
use master_fiber_agent::CrossBorderSwapExecutor;
use mesh_core::compliance::ComplianceAuditEnvelope;
use mesh_core::currency::{AssetRegistryHub, CurrencyAssetConfig, SpotMarketRate};
use uuid::Uuid;

async fn seed_e2e_registry() -> AssetRegistryHub {
    let registry = AssetRegistryHub::new();

    registry
        .introduce_currency_asset(CurrencyAssetConfig {
            iso_code: "TZS".to_string(),
            country_name: "Tanzania".to_string(),
            atomic_decimals: 8,
            udt_code_hash: "0x878fcc6f1f08d".to_string(),
            udt_args: "tzs_l1_args".to_string(),
            macro_velocity_limit_24h: 500_000_000.0,
        })
        .await;

    registry
        .introduce_currency_asset(CurrencyAssetConfig {
            iso_code: "KES".to_string(),
            country_name: "Kenya".to_string(),
            atomic_decimals: 8,
            udt_code_hash: "0x224a99bca11".to_string(),
            udt_args: "kes_l1_args".to_string(),
            macro_velocity_limit_24h: 15_000_000.0,
        })
        .await;

    registry
        .apply_spot_market_rate(SpotMarketRate {
            pair_id: Uuid::new_v4(),
            base_currency: "TZS".to_string(),
            quote_currency: "KES".to_string(),
            exchange_rate: 0.05,
            last_oracle_update: chrono::Utc::now().timestamp() as u64,
            regulatory_spread_markup: 0.0,
        })
        .await;

    registry
}

#[tokio::test]
async fn test_full_b2b_remittance_lifecycle() {
    let registry = seed_e2e_registry().await;

    let (compliance_tx, mut compliance_rx) = broadcast::channel::<ComplianceAuditEnvelope>(100);
    let app_state = app_state_with_registry(compliance_tx, registry);

    let executor = CrossBorderSwapExecutor::from_state(app_state);

    let principal_fiat = 2_500_000.0;
    let target_pubkey = "03abc123def4567890abcdef";

    let result = executor
        .execute_atomic_b2b_remittance(104, "TZS", "KES", principal_fiat, target_pubkey)
        .await;

    assert!(
        result.is_ok(),
        "The B2B Remittance pipeline failed: {:?}",
        result.err()
    );

    let audit_log = compliance_rx
        .recv()
        .await
        .expect("Compliance Hub failed to stream telemetry to the regulator bus.");

    assert_eq!(audit_log.central_bank_feed.volume_fiat_value, principal_fiat);
    assert_eq!(
        audit_log.revenue_authority_feed.calculated_sovereign_tax_levy,
        2_500.0
    );
    assert_eq!(
        audit_log.revenue_authority_feed.transaction_type,
        "B2B_REMITTANCE"
    );
}
