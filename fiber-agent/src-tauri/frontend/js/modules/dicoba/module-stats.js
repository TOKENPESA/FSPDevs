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
      snapshot.dicobaMemberId || "Your ID in this savings group",
      { trend: true },
    ),
    metricCell("Group", snapshot.vaultName, "Active savings group", { trend: true }),
    metricCell("Your shares", formatShannons(snapshot.memberShares), "Shares you own in the group"),
    metricCell(
      "Trust score",
      `${snapshot.reputationScore}`,
      snapshot.reputationScore >= 90 ? "High trust — larger borrow limit" : "Standard trust level",
      { trend: snapshot.reputationScore >= 90 },
    ),
    metricCell(
      "Group pool",
      formatShannons(snapshot.poolSharesShannons),
      formatFiatFromShannons(snapshot.poolSharesShannons, snapshot.conversionRate),
    ),
    metricCell(
      "Social fund",
      formatShannons(snapshot.poolSocialFundShannons),
      formatFiatFromShannons(snapshot.poolSocialFundShannons, snapshot.conversionRate),
    ),
    metricCell(
      "Contributions",
      formatCount(snapshot.contributions, { label: "receipts" }),
      "Contribution receipts recorded",
    ),
    metricCell("Groups tracked", formatCount(snapshot.totalVaults, { label: "groups" }), "Savings groups on record"),
  ].join("");

  return metricSection("Savings overview", cells, {
    hint: "Live group balances",
    actionHtml: `<button type="button" class="refresh-btn refresh-btn-inline" data-action="refresh-dicoba-stats">Refresh</button>`,
  });
}

/** @param {DicobaStatsSnapshot} snapshot @returns {string} */
export function renderLoanStats(snapshot) {
  const cells = [
    metricCell(
      "Your member ID",
      shortMemberId(snapshot.dicobaMemberId),
      snapshot.dicobaMemberId || "Share this when you guarantee a loan",
      { trend: true },
    ),
    metricCell(
      "Max you can borrow",
      `${snapshot.maxBorrowFiat.toLocaleString()} TZS`,
      "Based on your shares and trust score",
      { trend: true },
    ),
    metricCell(
      "Interest rate",
      `${snapshot.interestMonthlyPct.toFixed(1)}% / mo`,
      "Monthly rate for this loan",
    ),
    metricCell(
      "Trust score",
      `${snapshot.reputationScore}`,
      snapshot.reputationScore >= 90 ? "High trust — up to 3× borrow" : "Standard — up to 1× borrow",
      { trend: snapshot.reputationScore >= 90 },
    ),
    metricCell("Your shares", formatShannons(snapshot.memberShares), "Used as loan security"),
    metricCell(
      "Locked for guarantees",
      formatShannons(snapshot.lockedAsGuarantorShannons),
      formatFiatFromShannons(snapshot.lockedAsGuarantorShannons, snapshot.conversionRate),
    ),
    metricCell("Group", snapshot.vaultName, "Linked savings group"),
  ].join("");

  return metricSection("Loan overview", cells, {
    hint: "Borrowing and guarantee status",
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
