use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FinancialInstitution {
    /// Bank Identifier Code (e.g., "CRDBTZTZ" for CRDB Tanzania).
    pub bic_fi: String,
    /// PAPSS participant ID.
    pub clearing_system_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveCurrencyAndAmount {
    /// ISO 4217 code (e.g., "TZS").
    pub currency: String,
    /// Macro settlement amount.
    pub value: f64,
}

/// ISO-20022 pacs.009 core financial institution transfer payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pacs009Transfer {
    pub message_id: String,
    pub creation_datetime: String,
    pub settlement_amount: ActiveCurrencyAndAmount,
    pub debtor_agent: FinancialInstitution,
    pub creditor_agent: FinancialInstitution,
    /// e.g. "FSP_PROTOCOL_MACRO_REBALANCE_001"
    pub instruction_info: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PapssSettlementStatus {
    /// Pacs.002 status: accepted settlement completed.
    Settled,
    /// Pacs.002 status: pending liquidity check at Afreximbank.
    Pending,
    /// Pacs.002 status: rejected due to insufficient RTGS funds.
    Rejected(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PapssSettlementReceipt {
    pub original_message_id: String,
    pub papss_transaction_id: String,
    pub status: PapssSettlementStatus,
    pub timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacs009_transfer_round_trips_json() {
        let transfer = Pacs009Transfer {
            message_id: "FSP-PACS009-001".to_string(),
            creation_datetime: "2026-06-12T10:00:00Z".to_string(),
            settlement_amount: ActiveCurrencyAndAmount {
                currency: "TZS".to_string(),
                value: 50_000_000.0,
            },
            debtor_agent: FinancialInstitution {
                bic_fi: "CRDBTZTZ".to_string(),
                clearing_system_id: "PAPSS-TZ-CRDB".to_string(),
            },
            creditor_agent: FinancialInstitution {
                bic_fi: "KCBLKENX".to_string(),
                clearing_system_id: "PAPSS-KE-KCB".to_string(),
            },
            instruction_info: "FSP_PROTOCOL_MACRO_REBALANCE_001".to_string(),
        };

        let json = serde_json::to_string(&transfer).expect("serialize");
        let decoded: Pacs009Transfer = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.message_id, "FSP-PACS009-001");
        assert_eq!(decoded.settlement_amount.currency, "TZS");
    }

    #[test]
    fn papss_settlement_receipt_rejected_variant_round_trips() {
        let receipt = PapssSettlementReceipt {
            original_message_id: "FSP-PACS009-001".to_string(),
            papss_transaction_id: "PAPSS-TXN-8842".to_string(),
            status: PapssSettlementStatus::Rejected(
                "Insufficient RTGS liquidity at debtor agent".to_string(),
            ),
            timestamp: 1_700_000_000,
        };

        let json = serde_json::to_string(&receipt).expect("serialize");
        let decoded: PapssSettlementReceipt = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(decoded.status, PapssSettlementStatus::Rejected(_)));
    }
}
