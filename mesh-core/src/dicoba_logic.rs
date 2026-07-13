use uuid::Uuid;

use crate::jungukuu_types::{DicobaMember, JunguKuuVault};

/// Constitutional rules engine for DICOBA / JunguKuu collective savings vaults.
pub struct DicobaEngine;

impl DicobaEngine {
    /// Validates a weekly contribution bundle and returns the total shannons to route on-chain.
    pub fn process_weekly_contribution(
        vault: &mut JunguKuuVault,
        member_id: Uuid,
        shares_to_buy: u32,
        pay_social_fund: bool,
        fines_shannons: u64,
    ) -> Result<u64, String> {
        if vault.governance_lock.leader_pubkeys.is_empty() {
            return Err("Vault requires at least one leader pubkey".to_string());
        }

        if shares_to_buy == 0 && !pay_social_fund && fines_shannons == 0 {
            return Err(
                "Contribution must include shares, social fund payment, or fines".to_string(),
            );
        }

        let share_total = u64::from(shares_to_buy).saturating_mul(vault.share_price_shannons);
        let social = if pay_social_fund {
            vault.social_fund_flat_fee_shannons
        } else {
            0
        };
        let total_shannons = share_total
            .saturating_add(social)
            .saturating_add(fines_shannons);

        if let Some(member) = vault
            .members
            .iter_mut()
            .find(|member| member.member_id == member_id)
        {
            member.shares_owned = member.shares_owned.saturating_add(shares_to_buy);
            if pay_social_fund {
                member.social_fund_contributions_shannons = member
                    .social_fund_contributions_shannons
                    .saturating_add(social);
            }
            member.total_fines_paid_shannons = member
                .total_fines_paid_shannons
                .saturating_add(fines_shannons);
        } else {
            let mut member = DicobaMember::new(member_id, "");
            member.shares_owned = shares_to_buy;
            if pay_social_fund {
                member.social_fund_contributions_shannons = social;
            }
            member.total_fines_paid_shannons = fines_shannons;
            vault.members.push(member);
        }

        vault.pool_shares_shannons = vault.pool_shares_shannons.saturating_add(share_total);
        vault.pool_social_fund_shannons = vault
            .pool_social_fund_shannons
            .saturating_add(social);
        vault.pool_fines_and_interest_shannons = vault
            .pool_fines_and_interest_shannons
            .saturating_add(fines_shannons);

        Ok(total_shannons)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jungukuu_types::{CycleState, MultisigQuorum};

    fn sample_vault() -> JunguKuuVault {
        JunguKuuVault {
            vault_id: Uuid::new_v4(),
            group_name: "Test Dicoba".to_string(),
            cycle_start_timestamp: 1_700_000_000,
            cycle_end_timestamp: 1_703_000_000,
            cycle_state: CycleState::Active,
            base_asset_iso: "TZS".to_string(),
            share_price_shannons: 100_000,
            social_fund_flat_fee_shannons: 25_000,
            base_interest_rate_bps: 500,
            peak_interest_rate_bps: 2_500,
            pool_shares_shannons: 0,
            pool_social_fund_shannons: 0,
            pool_fines_and_interest_shannons: 0,
            governance_lock: MultisigQuorum {
                total_signers: 3,
                required_signatures: 2,
                leader_pubkeys: vec!["03leader_test".to_string()],
            },
            members: vec![],
            l1_cell_outpoint: "0xabc:0".to_string(),
        }
    }

    #[test]
    fn weekly_contribution_totals_shares_social_and_fines() {
        let member_id = Uuid::new_v4();
        let mut vault = sample_vault();

        let total = DicobaEngine::process_weekly_contribution(
            &mut vault,
            member_id,
            2,
            true,
            5_000,
        )
        .expect("valid contribution");

        assert_eq!(total, 230_000);
        assert_eq!(vault.pool_shares_shannons, 200_000);
        assert_eq!(vault.pool_social_fund_shannons, 25_000);
        assert_eq!(vault.pool_fines_and_interest_shannons, 5_000);
        assert_eq!(vault.members.len(), 1);
        assert_eq!(vault.members[0].shares_owned, 2);
    }

    #[test]
    fn weekly_contribution_rejects_empty_bundle() {
        let mut vault = sample_vault();
        let err = DicobaEngine::process_weekly_contribution(
            &mut vault,
            Uuid::new_v4(),
            0,
            false,
            0,
        )
        .expect_err("empty bundle");
        assert!(err.contains("shares, social fund"));
    }
}
