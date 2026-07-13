use mesh_core::types::{FeeCalculationBreakdown, FeeLayersConfig};

pub struct FeeCalculationEngine;

impl FeeCalculationEngine {
    /// Compiles a granular 3-tier cost matrix based on the desired physical withdrawal volume.
    pub fn compute_cash_out_breakdown(
        requested_fiat_withdrawal: f64,
        config: &FeeLayersConfig,
        estimated_l2_hop_count: u32,
        shannons_per_fiat_unit: f64,
    ) -> FeeCalculationBreakdown {
        let layer1_l2_routing_fee_fiat = (estimated_l2_hop_count as f64) * 0.50;

        let variable_commission =
            requested_fiat_withdrawal * (config.kiosk_proportional_ppm as f64 / 1_000_000.0);
        let layer2_kiosk_commission_fiat = config.kiosk_flat_commission + variable_commission;

        let layer3_sovereign_levy_fiat = requested_fiat_withdrawal * config.sovereign_levy_rate;

        let absolute_total_fiat_cost = requested_fiat_withdrawal
            + layer1_l2_routing_fee_fiat
            + layer2_kiosk_commission_fiat
            + layer3_sovereign_levy_fiat;

        let absolute_total_shannons = (absolute_total_fiat_cost * shannons_per_fiat_unit) as u64;

        FeeCalculationBreakdown {
            principal_fiat_amount: requested_fiat_withdrawal,
            layer1_l2_routing_fee_fiat,
            layer2_kiosk_commission_fiat,
            layer3_sovereign_levy_fiat,
            absolute_total_fiat_cost,
            absolute_total_shannons,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> FeeLayersConfig {
        FeeLayersConfig {
            kiosk_flat_commission: 500.0,
            kiosk_proportional_ppm: 10_000,
            sovereign_levy_rate: 0.001,
        }
    }

    #[test]
    fn compute_cash_out_breakdown_aggregates_three_fee_layers() {
        let breakdown = FeeCalculationEngine::compute_cash_out_breakdown(
            10_000.0,
            &sample_config(),
            4,
            38.0,
        );

        assert_eq!(breakdown.principal_fiat_amount, 10_000.0);
        assert_eq!(breakdown.layer1_l2_routing_fee_fiat, 2.0);
        assert_eq!(breakdown.layer2_kiosk_commission_fiat, 600.0);
        assert_eq!(breakdown.layer3_sovereign_levy_fiat, 10.0);
        assert_eq!(breakdown.absolute_total_fiat_cost, 10_612.0);
        assert_eq!(breakdown.absolute_total_shannons, 403_256);
    }
}
