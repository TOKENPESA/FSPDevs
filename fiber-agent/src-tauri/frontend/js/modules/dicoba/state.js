import { createLogger } from "../../../dashboard/logger.js";
import { dispatchToModule } from "../../sidecar-api.js";

const log = createLogger("dicoba");

export const LOCAL_MEMBER_ID = "";

export const vaultState = {
  groupName: "Mabibo Community Fund",
  vaultId: /** @type {string | null} */ (null),
  localMemberId: LOCAL_MEMBER_ID,
  memberShares: 40,
  sharePriceShannons: 100_000,
  conversionRate: 38,
  reputationScore: 100,
  lockedAsGuarantorShannons: 0,
  baseInterestBps: 500,
  poolSharesShannons: 15_000_000,
  poolSocialFundShannons: 250_000,
};

/** @param {HTMLElement} root @param {"savings" | "loan"} [panel] @returns {string} */
export function getSelectedVaultName(root, panel = "savings") {
  if (panel === "loan") {
    const select = /** @type {HTMLSelectElement | null} */ (
      root.querySelector("[data-dicoba-loan-vault]")
    );
    const custom = /** @type {HTMLInputElement | null} */ (
      root.querySelector("[data-dicoba-loan-vault-custom]")
    );
    if (select?.value === "__custom__") {
      return custom?.value?.trim() || vaultState.groupName;
    }
    return select?.value?.trim() || vaultState.groupName;
  }

  const vaultInput = /** @type {HTMLInputElement | null} */ (
    root.querySelector("[data-dicoba-vault-name]")
  );
  return vaultInput?.value?.trim() || vaultState.groupName;
}

export async function syncVaultIdentity(groupName = vaultState.groupName) {
  try {
    const context = /** @type {Record<string, string>} */ (
      await dispatchToModule("dicoba", "get_vault_context", {
        group_name: groupName,
      })
    );
    vaultState.groupName = context.group_name ?? groupName;
    vaultState.vaultId = context.vault_id ?? vaultState.vaultId;
    vaultState.localMemberId =
      context.local_member_id ?? vaultState.localMemberId;
    return context;
  } catch (error) {
    log.warn("[dicoba] vault identity sync failed:", error);
    return null;
  }
}

export function buildVaultConfig(groupName = vaultState.groupName) {
  const now = Math.floor(Date.now() / 1000);
  const memberId = vaultState.localMemberId || LOCAL_MEMBER_ID;
  return {
    vault_id: vaultState.vaultId ?? "00000000-0000-0000-0000-000000000000",
    group_name: groupName,
    cycle_start_timestamp: now,
    cycle_end_timestamp: now + 2_592_000,
    cycle_state: "Active",
    base_asset_iso: "TZS",
    share_price_shannons: vaultState.sharePriceShannons,
    social_fund_flat_fee_shannons: 25_000,
    base_interest_rate_bps: vaultState.baseInterestBps,
    peak_interest_rate_bps: 2_500,
    pool_shares_shannons: vaultState.poolSharesShannons,
    pool_social_fund_shannons: vaultState.poolSocialFundShannons,
    pool_fines_and_interest_shannons: 0,
    governance_lock: {
      total_signers: 3,
      required_signatures: 2,
      leader_pubkeys: ["03leader_test"],
    },
    members: [
      {
        member_id: memberId,
        public_key: "03local_member",
        shares_owned: vaultState.memberShares,
        social_fund_contributions_shannons: 250_000,
        total_fines_paid_shannons: 0,
        active_loan: null,
        locked_as_guarantor_shannons: vaultState.lockedAsGuarantorShannons,
        digital_reputation_score: vaultState.reputationScore,
      },
    ],
    l1_cell_outpoint: "0xabc123:0",
  };
}

/** @param {HTMLElement} root @param {"savings" | "loan"} [panel] */
export function syncVaultStateFromDom(root, panel = "savings") {
  const vaultName = getSelectedVaultName(root, panel);
  vaultState.groupName = vaultName;
  const member = buildVaultConfig(vaultName).members[0];
  vaultState.memberShares = member.shares_owned;
  vaultState.lockedAsGuarantorShannons = member.locked_as_guarantor_shannons;
  vaultState.reputationScore = member.digital_reputation_score;
}

export async function syncVaultTransparency(groupName = vaultState.groupName) {
  try {
    await syncVaultIdentity(groupName);
    const profile = /** @type {Record<string, number>} */ (
      await dispatchToModule("dicoba", "get_credit_profile", {
        vault_config: buildVaultConfig(groupName),
        member_id: vaultState.localMemberId,
        shannons_conversion_rate: vaultState.conversionRate,
      })
    );
    vaultState.reputationScore =
      profile.digital_reputation_score ?? vaultState.reputationScore;
    vaultState.baseInterestBps =
      profile.current_interest_rate_bps ?? vaultState.baseInterestBps;
  } catch (error) {
    log.warn("[dicoba] transparency sync fell back to local state:", error);
  }
}

export function computeLoanMetrics() {
  const grossValue = vaultState.memberShares * vaultState.sharePriceShannons;
  const accessibleValue = Math.max(
    0,
    grossValue - vaultState.lockedAsGuarantorShannons,
  );
  const leverage = vaultState.reputationScore >= 90 ? 3 : 1;
  const maxCapacityFiat =
    (accessibleValue * leverage) / vaultState.conversionRate;
  const interestMonthlyPct = vaultState.baseInterestBps / 100;

  return { maxCapacityFiat, interestMonthlyPct };
}
