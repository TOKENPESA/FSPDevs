import { createLogger } from "../../../dashboard/logger.js";

import { loadSidecarRuntime } from "../../sidecar-runtime.js";

import { formatCount, formatShannons, metricCell, metricSection } from "../../stats-ui.js";



const log = createLogger("fiat-bridge");

/**
 * @typedef {Object} FiatBridgeStatsSnapshot
 * @property {number} fiatEdgeTransactions
 * @property {number} edgePending
 * @property {number} edgeSettled
 * @property {number} edgeFailed
 * @property {boolean} mounted
 * @property {string} mfaStatus
 * @property {boolean} mfaControlConnected
 * @property {boolean} mfaReachable
 * @property {string} [mfaHost]
 * @property {string} [mfaWsUrl]
 * @property {number} localLiquidity
 * @property {number} queuedTelemetry
 */

/** @param {FiatBridgeStatsSnapshot} snapshot @returns {string} */
function mfaStatusHint(snapshot) {
  if (snapshot.mfaControlConnected) {
    return "Hub connected";
  }
  if (snapshot.mfaReachable) {
    return "Hub reachable — still signing in";
  }
  return "Can't reach the hub right now";
}



async function loadFiatBridgeSnapshot() {

  try {

    const runtime = await loadSidecarRuntime();

    return {

      fiatEdgeTransactions: runtime?.fiatEdgeTransactions ?? 0,

      edgePending: runtime?.edgePending ?? 0,

      edgeSettled: runtime?.edgeSettled ?? 0,

      edgeFailed: runtime?.edgeFailed ?? 0,

      mounted: runtime?.mountedModules?.includes("fiat_bridge") ?? false,

      mfaStatus: runtime?.mfaConnectionStatus ?? "unknown",

      mfaControlConnected: runtime?.mfaControlConnected === true,

      mfaReachable: runtime?.mfaReachable === true,

      mfaHost: runtime?.mfaHost,

      mfaWsUrl: runtime?.mfaWsUrl,

      localLiquidity: runtime?.totalLocalBalanceShannons ?? 0,

      queuedTelemetry: runtime?.queuedTelemetry ?? 0,

    };

  } catch (error) {

    log.warn("[fiat_bridge] module stats unavailable:", error);

    return {

      fiatEdgeTransactions: 0,

      edgePending: 0,

      edgeSettled: 0,

      edgeFailed: 0,

      mounted: false,

      mfaStatus: "unknown",

      mfaControlConnected: false,

      mfaReachable: false,

      mfaHost: "—",

      mfaWsUrl: "—",

      localLiquidity: 0,

      queuedTelemetry: 0,

    };

  }

}



/** @param {FiatBridgeStatsSnapshot} snapshot @returns {string} */
export function renderFiatBridgeStats(snapshot) {

  const cells = [

    metricCell(
      "Deposits",
      formatCount(snapshot.fiatEdgeTransactions, { label: "records" }),
      "Cash deposits and transfers recorded",
    ),
    metricCell(
      "Pending",
      formatCount(snapshot.edgePending, { label: "pending" }),
      "Waiting for confirmation",
    ),
    metricCell(
      "Completed",
      formatCount(snapshot.edgeSettled, { label: "completed" }),
      "Confirmed transfers",
      { trend: snapshot.edgeSettled > 0 },
    ),
    metricCell(
      "Failed",
      formatCount(snapshot.edgeFailed, { label: "failed" }),
      "Could not complete",
    ),
    metricCell(
      "Cash desk",
      snapshot.mounted ? "Ready" : "Unavailable",
      mfaStatusHint(snapshot),
      { trend: snapshot.mounted && snapshot.mfaControlConnected },
    ),
    metricCell(
      "You can send",
      formatShannons(snapshot.localLiquidity),
      "Balance available to send",
      { trend: snapshot.localLiquidity > 0 },
    ),
    metricCell(
      "Waiting to sync",
      formatCount(snapshot.queuedTelemetry, { label: "updates" }),
      "Saved until the hub reconnects",
    ),
  ].join("");

  return metricSection("Cash & mobile money", cells, {
    hint: "Deposits, cash on hand, and hub sync",
    actionHtml: `<button type="button" class="refresh-btn refresh-btn-inline" data-action="refresh-fiat-stats">Refresh</button>`,
  });
}



/** @param {HTMLElement} root */
export async function mountFiatBridgeStats(root) {

  const host = root.querySelector("[data-module-stats-host]");

  if (!host) return;



  const paint = async () => {

    const snapshot = await loadFiatBridgeSnapshot();

    host.innerHTML = renderFiatBridgeStats(snapshot);

    host.querySelector('[data-action="refresh-fiat-stats"]')?.addEventListener("click", () => {

      void paint();

    });

  };



  await paint();

  return paint;

}


