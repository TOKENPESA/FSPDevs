use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComplianceVerdict {
    /// Safe transaction that matches capital rules and settles immediately.
    ClearedClean,
    /// Transaction flags anomaly thresholds but passes unblocked for post-audit tracking.
    AuditFlagged,
    /// Sovereign intervention: transaction blocked instantly by macroprudential boundary rules.
    SovereignBlocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CentralBankMacroTelemetry {
    pub clearing_node_id: Uuid,
    pub source_corridor_iso: String,
    pub destination_corridor_iso: String,
    pub volume_fiat_value: f64,
    pub rolling_24h_corridor_total: f64,
    /// Real-time consumption ratio of overall national allowance caps.
    pub macro_velocity_percent: f64,
    /// Masked token protecting user identity from raw disclosure.
    pub masked_kyc_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RevenueAuthorityTaxTelemetry {
    pub originating_agent_id: u16,
    /// e.g. "CASH_IN", "CASH_OUT", "B2B_REMITTANCE"
    pub transaction_type: String,
    pub gross_value_fiat: f64,
    pub agent_commission_earned: f64,
    /// Exact real-time tax levy collected.
    pub calculated_sovereign_tax_levy: f64,
    /// Dynamic reference pointing to active regional tax provisions.
    pub revenue_tax_code_reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComplianceAuditEnvelope {
    pub audit_id: Uuid,
    pub sequence_index: u64,
    pub transaction_timestamp: u64,
    pub central_bank_feed: CentralBankMacroTelemetry,
    pub revenue_authority_feed: RevenueAuthorityTaxTelemetry,
    pub final_verdict: ComplianceVerdict,
    pub administrative_lock_signature: Option<String>,
}

/// Placeholder structure to guarantee cross-crate compilation of in-flight L2 swap instances.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntentSwapOrder {
    pub swap_id: Uuid,
    pub infrastructure_channel_id: Uuid,
    pub counterparty_pubkey: String,
    pub target_asset_symbol: String,
    pub expiration_locktime: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compliance_audit_envelope_round_trips_json() {
        let envelope = ComplianceAuditEnvelope {
            audit_id: Uuid::new_v4(),
            sequence_index: 42,
            transaction_timestamp: 1_700_000_000,
            central_bank_feed: CentralBankMacroTelemetry {
                clearing_node_id: Uuid::new_v4(),
                source_corridor_iso: "TZS".to_string(),
                destination_corridor_iso: "KES".to_string(),
                volume_fiat_value: 50_000.0,
                rolling_24h_corridor_total: 250_000.0,
                macro_velocity_percent: 0.25,
                masked_kyc_token: "kyc_****9012".to_string(),
            },
            revenue_authority_feed: RevenueAuthorityTaxTelemetry {
                originating_agent_id: 44,
                transaction_type: "CASH_OUT".to_string(),
                gross_value_fiat: 50_000.0,
                agent_commission_earned: 600.0,
                calculated_sovereign_tax_levy: 50.0,
                revenue_tax_code_reference: "TZ-VAT-2026-A".to_string(),
            },
            final_verdict: ComplianceVerdict::ClearedClean,
            administrative_lock_signature: None,
        };

        let json = serde_json::to_string(&envelope).expect("serialize");
        let decoded: ComplianceAuditEnvelope = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.final_verdict, ComplianceVerdict::ClearedClean);
        assert_eq!(decoded.revenue_authority_feed.transaction_type, "CASH_OUT");
    }
}
