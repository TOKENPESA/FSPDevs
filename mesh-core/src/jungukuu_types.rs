use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CycleState {
    Active,
    Liquidating,
    SharedOut,
}

/// Represents a cryptographic lock placed on a guarantor's shares
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuarantorStake {
    pub guarantor_member_id: Uuid,
    pub locked_shannons: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveLoan {
    pub loan_id: Uuid,
    pub principal_shannons: u64,
    pub interest_owed_shannons: u64,
    pub due_timestamp: u64,
    /// The guarantors who have mathematically staked their own shares against this loan
    pub backing_guarantors: Vec<GuarantorStake>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DicobaMember {
    pub member_id: Uuid,
    pub public_key: String,

    // Core DICOBA Accounting
    pub shares_owned: u32,
    pub social_fund_contributions_shannons: u64,
    pub total_fines_paid_shannons: u64,

    // Risk & Credit Mechanics
    pub active_loan: Option<ActiveLoan>,
    /// Tracks shares locked as a guarantor for someone else's loan.
    /// These cannot be withdrawn or used to borrow until the backed loan is cleared.
    pub locked_as_guarantor_shannons: u64,
    pub digital_reputation_score: u16, // Starts at 100, drops on late payments
}

impl DicobaMember {
    pub fn new(member_id: Uuid, public_key: impl Into<String>) -> Self {
        Self {
            member_id,
            public_key: public_key.into(),
            shares_owned: 0,
            social_fund_contributions_shannons: 0,
            total_fines_paid_shannons: 0,
            active_loan: None,
            locked_as_guarantor_shannons: 0,
            digital_reputation_score: 100,
        }
    }

    /// Determines borrowing capacity based on UNLOCKED shares and digital reputation
    pub fn maximum_borrowing_capacity(&self, share_price_shannons: u64) -> u64 {
        let gross_value = self.shares_owned as u64 * share_price_shannons;
        let accessible_value = gross_value.saturating_sub(self.locked_as_guarantor_shannons);

        // Base is 3x, but scales down if reputation drops
        let leverage_multiplier = if self.digital_reputation_score >= 90 {
            3
        } else {
            1
        };

        accessible_value * leverage_multiplier
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultisigQuorum {
    pub total_signers: u8,
    /// e.g. 3 out of 5 leaders must sign to unlock the L1 cell.
    pub required_signatures: u8,
    pub leader_pubkeys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JunguKuuVault {
    pub vault_id: Uuid,
    pub group_name: String,

    // Cycle Timeframes
    pub cycle_start_timestamp: u64,
    pub cycle_end_timestamp: u64,
    pub cycle_state: CycleState,

    // Constitution Financial Rules
    pub base_asset_iso: String, // E.g., RUSD (Stablecoin mapped to local fiat)
    pub share_price_shannons: u64,
    pub social_fund_flat_fee_shannons: u64,

    // Algorithmic Interest Parameters
    pub base_interest_rate_bps: u16,
    pub peak_interest_rate_bps: u16,

    // Aggregated Vault Pools
    pub pool_shares_shannons: u64,
    pub pool_social_fund_shannons: u64,
    pub pool_fines_and_interest_shannons: u64,

    pub governance_lock: MultisigQuorum,
    pub members: Vec<DicobaMember>,
    pub l1_cell_outpoint: String,
}

impl JunguKuuVault {
    /// Dynamically calculates the current interest rate based on pool utilization
    pub fn current_utilization_interest_rate(&self) -> u16 {
        let total_borrowed: u64 = self
            .members
            .iter()
            .filter_map(|m| m.active_loan.as_ref())
            .map(|l| l.principal_shannons)
            .sum();

        if self.pool_shares_shannons == 0 {
            return self.base_interest_rate_bps;
        }

        let utilization_ratio = total_borrowed as f64 / self.pool_shares_shannons as f64;

        // Scales interest up as liquidity gets tighter
        let dynamic_rate = self.base_interest_rate_bps as f64
            + (utilization_ratio * (self.peak_interest_rate_bps - self.base_interest_rate_bps) as f64);

        dynamic_rate as u16
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicroContributionReceipt {
    pub transaction_id: Uuid,
    pub member_id: Uuid,
    pub vault_id: Uuid,
    pub amount_shannons: u64,
    pub timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_vault() -> JunguKuuVault {
        let member_id = Uuid::new_v4();
        let vault_id = Uuid::new_v4();

        JunguKuuVault {
            vault_id,
            group_name: "Mabibo Dicoba".to_string(),
            cycle_start_timestamp: 1_700_000_000,
            cycle_end_timestamp: 1_703_000_000,
            cycle_state: CycleState::Active,
            base_asset_iso: "TZS".to_string(),
            share_price_shannons: 100_000,
            social_fund_flat_fee_shannons: 25_000,
            base_interest_rate_bps: 500,
            peak_interest_rate_bps: 2_500,
            pool_shares_shannons: 12_500_000,
            pool_social_fund_shannons: 250_000,
            pool_fines_and_interest_shannons: 0,
            governance_lock: MultisigQuorum {
                total_signers: 5,
                required_signatures: 3,
                leader_pubkeys: vec![
                    "03leader_a".to_string(),
                    "03leader_b".to_string(),
                    "03leader_c".to_string(),
                ],
            },
            members: vec![DicobaMember {
                member_id,
                public_key: "03member_001".to_string(),
                shares_owned: 25,
                social_fund_contributions_shannons: 250_000,
                total_fines_paid_shannons: 0,
                active_loan: None,
                locked_as_guarantor_shannons: 0,
                digital_reputation_score: 100,
            }],
            l1_cell_outpoint: "0xabc123:0".to_string(),
        }
    }

    #[test]
    fn jungukuu_vault_round_trips_json() {
        let vault = sample_vault();
        let vault_id = vault.vault_id;

        let json = serde_json::to_string(&vault).expect("serialize");
        let decoded: JunguKuuVault = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.vault_id, vault_id);
        assert_eq!(decoded.base_asset_iso, "TZS");
        assert_eq!(decoded.governance_lock.required_signatures, 3);
        assert_eq!(decoded.share_price_shannons, 100_000);
    }

    #[test]
    fn member_borrowing_capacity_scales_with_reputation() {
        let mut member = DicobaMember::new(Uuid::new_v4(), "03member");
        member.shares_owned = 10;
        member.digital_reputation_score = 95;
        assert_eq!(member.maximum_borrowing_capacity(100_000), 3_000_000);

        member.digital_reputation_score = 70;
        assert_eq!(member.maximum_borrowing_capacity(100_000), 1_000_000);
    }

    #[test]
    fn vault_interest_rate_rises_with_utilization() {
        let mut vault = sample_vault();
        assert_eq!(vault.current_utilization_interest_rate(), 500);

        vault.members[0].active_loan = Some(ActiveLoan {
            loan_id: Uuid::new_v4(),
            principal_shannons: 6_250_000,
            interest_owed_shannons: 0,
            due_timestamp: 1_702_000_000,
            backing_guarantors: vec![],
        });

        assert!(vault.current_utilization_interest_rate() > vault.base_interest_rate_bps);
    }

    #[test]
    fn micro_contribution_receipt_round_trips_json() {
        let receipt = MicroContributionReceipt {
            transaction_id: Uuid::new_v4(),
            member_id: Uuid::new_v4(),
            vault_id: Uuid::new_v4(),
            amount_shannons: 500_000,
            timestamp: 1_700_000_100,
        };

        let json = serde_json::to_string(&receipt).expect("serialize");
        let decoded: MicroContributionReceipt = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.amount_shannons, 500_000);
    }
}
