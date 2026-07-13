import {
  executeCashInTransaction,
  onTauriEvent,
  triggerManualFiatRebalance,
} from "../../sidecar-api.js";
import {
  applyRuntimeToFloatPanel,
  bindShannonsFiatSync,
  conversionRate,
  defaultCustomerPubkey,
  loadSidecarRuntime,
  markUserEdited,
  SIDECAR_RUNTIME_EVENT,
} from "../../sidecar-runtime.js";
import { escapeHtml } from "../../dom-security.js";
import { errorMessage } from "../../../packages/fsp-ui-types/errors.js";
import {
  parseAtomicInt,
  parseFiatMinorUnits,
  shannonsToFiatMinor,
} from "../../../dashboard/money.js";
import { mountFiatBridgeStats } from "./module-stats.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarPanel} SidecarPanel */
/** @typedef {import("../../../../../../dashboard/types.js").SidecarRuntimeStats} SidecarRuntimeStats */

/**
 * @param {Record<string, unknown>} tx
 * @returns {number}
 */
function totalAtomicFromTx(tx) {
  const capacities = tx.capacities;
  if (Array.isArray(capacities)) {
    return capacities.reduce((sum, cap) => {
      const amount = Number(
        cap && typeof cap === "object" && "amount_atomic" in cap
          ? cap.amount_atomic
          : 0,
      );
      return sum + (Number.isFinite(amount) ? amount : 0);
    }, 0);
  }
  const legacy = Number(tx.amount_atomic);
  return Number.isFinite(legacy) ? legacy : 0;
}

/** @type {SidecarPanel} */
export const floatPanel = {
  id: "fiat-bridge-float",
  title: "Mobile Float Operations",
  navLabel: "Float & Cash-In",
  navIcon: "float",
  badge: "fiat_bridge / process_cash_in",
  navDescription:
    "Monitor telco float reserves, process cash-in, and dispatch crisis clearing",
  render() {
    return `
      <div class="module-workspace-inner" data-panel="fiat-bridge-float">
        <div class="workspace-card">
          <h2 class="workspace-section-title">Float Reserve</h2>
          <div class="input-group">
            <label>Active Telco Float Reserve (TZS)</label>
            <input type="number" data-float-reserve value="50000">
          </div>
          <div class="liquidity-bar" aria-hidden="true">
            <div class="liquidity-fill local" data-float-fill-local style="width:65%"></div>
            <div class="liquidity-fill remote" data-float-fill-remote style="width:35%"></div>
          </div>
          <p class="panel-hint" data-float-summary>Syncing float from FNN outbound liquidity…</p>
          <p class="panel-hint" data-critical-floor-hint>Critical floor: 50,000 TZS</p>
          <button type="button" class="primary-btn" data-action="float-rebalance">Run Float Crisis Check</button>
          <div class="receipt-log" data-clearing-log style="display:none;"></div>
        </div>

        <div class="workspace-card">
          <h2 class="workspace-section-title">Cash-In Routing</h2>
          <div class="input-group">
            <label>Customer FNN Pubkey</label>
            <input type="text" data-cash-in-pubkey placeholder="Synced from mesh peer…">
            <span class="panel-hint" data-mesh-peer-hint></span>
          </div>
          <div class="input-group">
            <label>Amount (Shannons / RUSD atomic)</label>
            <input type="number" data-cash-in-shannons value="1000000">
          </div>
          <div class="input-group">
            <label>Fiat Received (TZS)</label>
            <input type="number" data-cash-in-fiat value="38000">
          </div>
          <button type="button" class="primary-btn" data-action="cash-in-submit">Process Cash-In</button>
          <div class="receipt-log" data-cash-in-log style="display:none;"></div>
        </div>
      </div>
    `;
  },
  /** @param {HTMLElement} root */
  async mount(root) {
    const panel = root.querySelector('[data-panel="fiat-bridge-float"]');
    if (!(panel instanceof HTMLElement)) return () => {};

    const reserveInput = panel.querySelector("[data-float-reserve]");
    const fillLocal = panel.querySelector("[data-float-fill-local]");
    const fillRemote = panel.querySelector("[data-float-fill-remote]");
    const summary = panel.querySelector("[data-float-summary]");
    const log = panel.querySelector("[data-cash-in-log]");
    const clearingLog = panel.querySelector("[data-clearing-log]");
    const pubkeyInput = panel.querySelector("[data-cash-in-pubkey]");
    const shannonsInput = panel.querySelector("[data-cash-in-shannons]");
    const fiatInput = panel.querySelector("[data-cash-in-fiat]");

    if (
      !(reserveInput instanceof HTMLInputElement) ||
      !(fillLocal instanceof HTMLElement) ||
      !(fillRemote instanceof HTMLElement) ||
      !(summary instanceof HTMLElement) ||
      !(pubkeyInput instanceof HTMLInputElement) ||
      !(shannonsInput instanceof HTMLInputElement) ||
      !(fiatInput instanceof HTMLInputElement)
    ) {
      return () => {};
    }

    markUserEdited(reserveInput);
    markUserEdited(pubkeyInput);
    markUserEdited(shannonsInput);
    markUserEdited(fiatInput);

    let runtime = await loadSidecarRuntime({ force: true });
    bindShannonsFiatSync(shannonsInput, fiatInput, conversionRate(runtime));

    const syncFloatBar = (/** @type {SidecarRuntimeStats | null} */ activeRuntime = runtime) => {
      const currentFiatMinor = parseFiatMinorUnits(reserveInput.value, "fiat");
      const currentFiat = Number(shannonsToFiatMinor(currentFiatMinor, conversionRate(activeRuntime)));
      const localShannons = activeRuntime?.totalLocalBalanceShannons ?? 0;
      const remoteShannons = activeRuntime?.totalRemoteBalanceShannons ?? 0;
      const total = localShannons + remoteShannons || 1;
      const localPct = Math.min(Math.max((localShannons / total) * 100, 15), 85);
      fillLocal.style.width = `${localPct}%`;
      fillRemote.style.width = `${100 - localPct}%`;
      summary.textContent = `Local float: ${currentFiat.toLocaleString()} TZS · FNN outbound ${localShannons.toLocaleString()} shannons · inbound ${remoteShannons.toLocaleString()} shannons`;
    };

    const paintFromRuntime = (/** @type {SidecarRuntimeStats | null} */ activeRuntime) => {
      runtime = activeRuntime;
      applyRuntimeToFloatPanel(panel, activeRuntime);
      syncFloatBar(activeRuntime);
    };

    paintFromRuntime(runtime);
    const repaintStats = await mountFiatBridgeStats(root);

    reserveInput.addEventListener("input", () => syncFloatBar(runtime));

    const onRuntimeUpdated = (/** @type {Event} */ event) => {
      paintFromRuntime(/** @type {CustomEvent<SidecarRuntimeStats>} */ (event).detail);
      void repaintStats?.();
    };
    window.addEventListener(SIDECAR_RUNTIME_EVENT, onRuntimeUpdated);

    /** @param {string} line */
    const appendClearing = (line) => {
      if (!(clearingLog instanceof HTMLElement)) return;
      clearingLog.style.display = "block";
      const prefix = clearingLog.innerHTML ? `${clearingLog.innerHTML}<br>` : "";
      clearingLog.innerHTML = `${prefix}${escapeHtml(line)}`;
    };

    const unlistenFloat = await onTauriEvent("float-crisis", (/** @type {unknown} */ payload) => {
      appendClearing(`🚨 [FLOAT CRISIS] TelemetryPacket queued · ${typeof payload === "string" ? payload : JSON.stringify(payload)}`);
    });
    const unlistenDispatch = await onTauriEvent("clearing-dispatch", (payload) => {
      const record = /** @type {Record<string, unknown>} */ (payload ?? {});
      const status = record.status ?? "DISPATCHED";
      if (status === "MFA_OFFLINE") {
        appendClearing("📡 [REGIONAL CLEARING] Float-crisis posted — MFA offline, telemetry queued locally");
      } else {
        appendClearing(`✅ [REGIONAL CLEARING] MFA float-crisis intake accepted (status: ${status})`);
        appendClearing("🏦 [ENTERPRISE CLEARING] BalanceDepleted refuel handled by EnterpriseClearinghouse on supervisor");
      }
    });

    /** @param {Element | null} logEl @param {string} html */
    const setLogHtml = (logEl, html) => {
      if (!(logEl instanceof HTMLElement)) return;
      logEl.style.display = "block";
      logEl.innerHTML = html;
    };

    panel
      .querySelector('[data-action="float-rebalance"]')
      ?.addEventListener("click", async () => {
        runtime = await loadSidecarRuntime({ force: true });
        paintFromRuntime(runtime);
        const currentFiat = Number(
          shannonsToFiatMinor(parseFiatMinorUnits(reserveInput.value, "fiat"), conversionRate(runtime)),
        );
        setLogHtml(log, "⚙️ Evaluating float drain velocity against critical floor…");
        if (clearingLog instanceof HTMLElement) {
          clearingLog.style.display = "block";
          clearingLog.innerHTML = "Dispatching regional float-crisis telemetry to MFA…";
        }
        try {
          const message = await triggerManualFiatRebalance({
            currentFiat,
            digitalL2BalanceShannons: runtime?.totalLocalBalanceShannons ?? 0,
          });
          const isSafe = message.includes("within safe bounds");
          const iconLabel = isSafe ? "✓" : "⚠️";
          setLogHtml(log, `${iconLabel} ${escapeHtml(message)}`);
          await repaintStats?.();
        } catch (error) {
          setLogHtml(log, `❌ Float crisis check failed: ${escapeHtml(errorMessage(error))}`);
        }
      });

    panel
      .querySelector('[data-action="cash-in-submit"]')
      ?.addEventListener("click", async () => {
        runtime = await loadSidecarRuntime({ force: true });
        const customerPubkey =
          pubkeyInput.value.trim() || defaultCustomerPubkey(runtime);
        const amountShannons = Number(parseAtomicInt(shannonsInput.value, "shannons"));
        const fiatReceived = Number(
          shannonsToFiatMinor(parseFiatMinorUnits(fiatInput.value, "fiat"), conversionRate(runtime)),
        );

        if (!customerPubkey || !amountShannons) {
          setLogHtml(
            log,
            "❌ Customer pubkey and shannon amount are required. Sync runtime or enter a mesh peer pubkey.",
          );
          return;
        }

        if (!pubkeyInput.value.trim()) {
          pubkeyInput.value = customerPubkey;
        }

        setLogHtml(log, "📥 Routing cash-in through FNN loopback…");

        try {
          const tx = /** @type {Record<string, string | number>} */ (
            await executeCashInTransaction({
              customerPubkey,
              amountShannons,
              fiatReceived,
            })
          );
          const counterparty = String(tx.counterparty_pubkey ?? "");
          setLogHtml(
            log,
            `✅ <strong>Cash-In Recorded</strong><br>Tx: ${escapeHtml(String(tx.tx_id))}<br>Customer: ${escapeHtml(counterparty.slice(0, 12))}…<br>Amount: ${escapeHtml(totalAtomicFromTx(tx).toLocaleString())} shannons · ${escapeHtml(Number(tx.fiat_amount).toLocaleString())} TZS`,
          );
          runtime = await loadSidecarRuntime({ force: true });
          paintFromRuntime(runtime);
          await repaintStats?.();
        } catch (error) {
          setLogHtml(log, `❌ Cash-in failed: ${escapeHtml(errorMessage(error))}`);
        }
      });

    return () => {
      window.removeEventListener(
        SIDECAR_RUNTIME_EVENT,
        /** @type {EventListener} */ (onRuntimeUpdated),
      );
      unlistenFloat?.();
      unlistenDispatch?.();
    };
  },
};
