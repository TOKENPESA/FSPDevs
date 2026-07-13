import { createLogger } from "../../../dashboard/logger.js";
import { getSidecarStats } from "../../sidecar-api.js";
import {
  formatCount,
  formatFiatFromShannons,
  formatShannons,
  metricCell,
  metricSection,
  shortMemberId,
} from "../../stats-ui.js";
import {
  computeLoanMetrics,
  syncVaultIdentity,
  syncVaultTransparency,
  vaultState,
} from "./state.js";

const log = createLogger("dicoba");

/**
 * @typedef {Object} DicobaStatsSnapshot
 * @property {string} vaultName
 * @property {number} memberShares
 * @property {number} reputationScore
 * @property {number} maxBorrowFiat
 * @property {number} interestMonthlyPct
 * @property {number} poolSharesShannons
 * @property {number} poolSocialFundShannons
 * @property {number} lockedAsGuarantorShannons
 * @property {number} conversionRate
 * @property {number} contributions
 * @property {number} totalVaults
 * @property {string} dicobaMemberId
 */

async function loadDicobaSnapshot() {
  await syncVaultIdentity();
  await syncVaultTransparency();
  const { maxCapacityFiat, interestMonthlyPct } = computeLoanMetrics();

  let contributions = 0;
  let totalVaults = 0;
  let conversionRate = vaultState.conversionRate;
  let dicobaMemberId = vaultState.localMemberId;
  try {
    const runtime = await getSidecarStats();
    contributions = runtime?.dicobaContributions ?? 0;
    totalVaults = runtime?.dicobaVaultsTotal ?? 0;
    if (runtime?.fiatConversionRate) {
      conversionRate = runtime.fiatConversionRate;
      vaultState.conversionRate = conversionRate;
    }
    if (runtime?.dicobaMemberId) {
      dicobaMemberId = runtime.dicobaMemberId;
      vaultState.localMemberId = dicobaMemberId;
    }
  } catch (error) {
    log.warn("[dicoba] contribution stats unavailable:", error);
  }

  return {
    vaultName: vaultState.groupName,
    memberShares: vaultState.memberShares,
    reputationScore: vaultState.reputationScore,
    maxBorrowFiat: maxCapacityFiat,
    interestMonthlyPct,
    poolSharesShannons: vaultState.poolSharesShannons,
    poolSocialFundShannons: vaultState.poolSocialFundShannons,
    lockedAsGuarantorShannons: vaultState.lockedAsGuarantorShannons,
    conversionRate,
    contributions,
    totalVaults,
    dicobaMemberId,
  };
}

/** @param {DicobaStatsSnapshot} snapshot @returns {string} */
export function renderSavingsStats(snapshot) {
  const cells = [
    metricCell(
      "Your member ID",
      shortMemberId(snapshot.dicobaMemberId),
      snapshot.dicobaMemberId || "DiCoBa identity for this agent",
      { trend: true },
    ),
    metricCell("Vault", snapshot.vaultName, "Active JunguKuu group vault", { trend: true }),
    metricCell("Member Shares", formatShannons(snapshot.memberShares), "Owned cooperative shares"),
    metricCell(
      "Reputation",
      `${snapshot.reputationScore}`,
      snapshot.reputationScore >= 90 ? "High-trust leverage tier" : "Standard leverage tier",
      { trend: snapshot.reputationScore >= 90 },
    ),
    metricCell(
      "Pool Shares",
      formatShannons(snapshot.poolSharesShannons),
      formatFiatFromShannons(snapshot.poolSharesShannons, snapshot.conversionRate),
    ),
    metricCell(
      "Social Fund",
      formatShannons(snapshot.poolSocialFundShannons),
      formatFiatFromShannons(snapshot.poolSocialFundShannons, snapshot.conversionRate),
    ),
    metricCell(
      "Contributions",
      formatCount(snapshot.contributions, { label: "receipts" }),
      "Micro-contribution receipts",
    ),
    metricCell("Total Vaults", formatCount(snapshot.totalVaults, { label: "vaults" }), "Recorded JunguKuu vaults"),
  ].join("");

  return metricSection("DICOBA Savings", cells, {
    hint: "Live vault transparency",
    actionHtml: `<button type="button" class="refresh-btn refresh-btn-inline" data-action="refresh-dicoba-stats">Refresh</button>`,
  });
}

/** @param {DicobaStatsSnapshot} snapshot @returns {string} */
export function renderLoanStats(snapshot) {
  const cells = [
    metricCell(
      "Your member ID",
      shortMemberId(snapshot.dicobaMemberId),
      snapshot.dicobaMemberId || "Share this when you are the guarantor",
      { trend: true },
    ),
    metricCell(
      "Max Borrowing",
      `${snapshot.maxBorrowFiat.toLocaleString()} TZS`,
      "Algorithmic capacity",
      { trend: true },
    ),
    metricCell(
      "Interest Rate",
      `${snapshot.interestMonthlyPct.toFixed(1)}% / mo`,
      "Utilization-based APR",
    ),
    metricCell(
      "Reputation",
      `${snapshot.reputationScore}`,
      snapshot.reputationScore >= 90 ? "3x leverage tier" : "1x leverage tier",
      { trend: snapshot.reputationScore >= 90 },
    ),
    metricCell("Member Shares", formatShannons(snapshot.memberShares), "Collateral base"),
    metricCell(
      "Guarantor Lock",
      formatShannons(snapshot.lockedAsGuarantorShannons),
      formatFiatFromShannons(snapshot.lockedAsGuarantorShannons, snapshot.conversionRate),
    ),
    metricCell("Vault", snapshot.vaultName, "Linked JunguKuu group"),
  ].join("");

  return metricSection("Smart Loan", cells, {
    hint: "Guarantor staking metrics",
    actionHtml: `<button type="button" class="refresh-btn refresh-btn-inline" data-action="refresh-dicoba-stats">Refresh</button>`,
  });
}

/**
 * @param {HTMLElement} root
 * @param {string} _panelSelector
 * @param {(snapshot: DicobaStatsSnapshot) => string} renderStats
 */
export async function mountDicobaStats(root, _panelSelector, renderStats) {
  const host = root.querySelector("[data-module-stats-host]");
  if (!host) return;

  const paint = async () => {
    const snapshot = await loadDicobaSnapshot();
    host.innerHTML = renderStats(snapshot);
    host.querySelector('[data-action="refresh-dicoba-stats"]')?.addEventListener("click", () => {
      void paint();
    });
  };

  await paint();
  return paint;
}
