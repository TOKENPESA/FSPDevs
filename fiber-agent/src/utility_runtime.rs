use mesh_core::types::EdgeTransaction;

pub struct UtilityRuntime {
    pub flow_rate_units_per_shannon: f64,
}

impl UtilityRuntime {
    /// Grants access to physical resources proportional to the confirmed micro-payment.
    pub fn dispense_resource(&self, payment: &EdgeTransaction) -> f64 {
        let granted_duration =
            (payment.total_atomic() as f64) * self.flow_rate_units_per_shannon;

        log::info!(
            "[IOT] Physical pump relay: ENABLED for {} seconds",
            granted_duration
        );
        // Drive GPIO pin high for `granted_duration`.

        granted_duration
    }
}

#[cfg(test)]
mod tests {
    use mesh_core::types::{EdgeTxType, L2Asset, SingleCapacityParams};
    use uuid::Uuid;

    use super::*;

    fn sample_payment(amount_atomic: u64) -> EdgeTransaction {
        EdgeTransaction::single_capacity(SingleCapacityParams {
            tx_id: Uuid::new_v4(),
            agent_id: 1,
            tx_type: EdgeTxType::CashIn,
            asset: L2Asset::RusdStablecoin,
            amount_atomic,
            fiat_amount: 1_000.0,
            counterparty_pubkey: "03customer".to_string(),
            payment_hash: Some("0xpay".to_string()),
            preimage: None,
            timestamp: 1_700_000_000,
            is_synchronized: true,
        })
    }

    #[test]
    fn dispense_resource_scales_by_flow_rate() {
        let runtime = UtilityRuntime {
            flow_rate_units_per_shannon: 0.001,
        };
        let duration = runtime.dispense_resource(&sample_payment(1_000_000));
        assert!((duration - 1_000.0).abs() < f64::EPSILON);
    }
}
