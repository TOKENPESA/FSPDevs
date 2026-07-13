use mesh_core::types::FeeLayersConfig;

pub struct MicropaymentEngine;

impl MicropaymentEngine {
    /// Safely routes fractional sub-fiat contributions to the community vault without flat-fee destruction.
    pub fn calculate_dicoba_contribution_fee(
        contribution_fiat: f64,
        routing_config: &FeeLayersConfig,
        micropayment_threshold_fiat: f64,
    ) -> f64 {
        let applicable_base_fee = if contribution_fiat <= micropayment_threshold_fiat {
            0.0
        } else {
            routing_config.kiosk_flat_commission
        };

        let fractional_fee =
            contribution_fiat * (routing_config.kiosk_proportional_ppm as f64 / 1_000_000.0);

        let total_routing_friction = applicable_base_fee + fractional_fee;
        let maximum_allowable_friction = contribution_fiat * 0.005;

        if total_routing_friction > maximum_allowable_friction {
            maximum_allowable_friction
        } else {
            total_routing_friction
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
    fn micro_contribution_waives_flat_fee_below_threshold() {
        let fee = MicropaymentEngine::calculate_dicoba_contribution_fee(
            1_000.0,
            &sample_config(),
            2_000.0,
        );
        // 1000 * (10000/1e6) = 10 TZS, under 0.5% cap (5 TZS) -> capped at 5
        assert_eq!(fee, 5.0);
    }

    #[test]
    fn large_contribution_applies_flat_plus_proportional_capped_at_half_percent() {
        let fee = MicropaymentEngine::calculate_dicoba_contribution_fee(
            100_000.0,
            &sample_config(),
            2_000.0,
        );
        // 500 + 1000 = 1500, cap = 500 -> 500
        assert_eq!(fee, 500.0);
    }

    #[test]
    fn mid_contribution_uses_proportional_only_when_flat_waived() {
        let fee = MicropaymentEngine::calculate_dicoba_contribution_fee(
            5_000.0,
            &sample_config(),
            10_000.0,
        );
        // flat waived, 5000 * 1% = 50, cap = 25 -> 25
        assert_eq!(fee, 25.0);
    }
}
