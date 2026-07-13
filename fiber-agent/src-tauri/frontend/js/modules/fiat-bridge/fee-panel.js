import { createLogger } from "../../../dashboard/logger.js";
import { calculateInvoicePreview } from "../../sidecar-api.js";
import {
  applyRuntimeToFeePanel,
  loadSidecarRuntime,
  markUserEdited,
  SIDECAR_RUNTIME_EVENT,
} from "../../sidecar-runtime.js";
import { parseFiatMinorUnits } from "../../../dashboard/money.js";
import { mountFiatBridgeStats } from "./module-stats.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarPanel} SidecarPanel */
/** @typedef {import("../../../../../../dashboard/types.js").SidecarRuntimeStats} SidecarRuntimeStats */

const log = createLogger("fiat-bridge");

/** @type {SidecarPanel} */
export const feePanel = {
  id: "fiat-bridge-fees",
  title: "Dynamic Fee Estimation Engine",
  navLabel: "Fee Estimation",
  navIcon: "fees",
  badge: "fiat_bridge / calculate_invoice_preview",
  navDescription:
    "Preview routing, kiosk, and sovereign levy deductions before cash-out",
  render() {
    return `
      <div class="module-workspace-inner" data-panel="fiat-bridge-fees">
        <div class="workspace-card">
          <div class="input-group">
            <label>Target Cash-Out Volume (Fiat)</label>
            <input type="number" data-fiat-volume value="50000">
          </div>
          <div class="fee-breakdown">
            <div class="fee-line"><span>L1/L2 Channel Routing Fee:</span><strong data-fee-l1>0.00</strong></div>
            <div class="fee-line"><span>Kiosk Commission:</span><strong data-fee-l2>0.00</strong></div>
            <div class="fee-line"><span>Sovereign Tax Levy (0.1%):</span><strong data-fee-l3 style="color:var(--accent);">0.00</strong></div>
            <hr class="workspace-divider">
            <div class="fee-line fee-line--total">
              <span>Total Deducted Invoice Amount:</span>
              <strong data-fee-total style="color:var(--accent);">0.00</strong>
            </div>
          </div>
        </div>
      </div>
    `;
  },
  /** @param {HTMLElement} root */
  async mount(root) {
    const panel = root.querySelector('[data-panel="fiat-bridge-fees"]');
    if (!(panel instanceof HTMLElement)) return () => {};

    const volumeInput = panel.querySelector("[data-fiat-volume]");
    if (!(volumeInput instanceof HTMLInputElement)) return () => {};

    markUserEdited(volumeInput);

    let runtime = await loadSidecarRuntime();
    applyRuntimeToFeePanel(panel, runtime);

    const repaintStats = await mountFiatBridgeStats(root);

    const refresh = async () => {
      const volume = Number(parseFiatMinorUnits(volumeInput.value, "fiat"));
      try {
        const breakdown = await calculateInvoicePreview({
          targetFiat: volume,
          flatCommission: 500.0,
          proportionalPpm: 10000,
          sovereignLevy: 0.001,
        });
        const l1 = panel.querySelector("[data-fee-l1]");
        const l2 = panel.querySelector("[data-fee-l2]");
        const l3 = panel.querySelector("[data-fee-l3]");
        const total = panel.querySelector("[data-fee-total]");
        if (l1) l1.textContent = breakdown.layer1_l2_routing_fee_fiat.toFixed(2);
        if (l2) l2.textContent = breakdown.layer2_kiosk_commission_fiat.toFixed(2);
        if (l3) l3.textContent = breakdown.layer3_sovereign_levy_fiat.toFixed(2);
        const totalDeductions =
          breakdown.layer1_l2_routing_fee_fiat +
          breakdown.layer2_kiosk_commission_fiat +
          breakdown.layer3_sovereign_levy_fiat;
        if (total) {
          total.textContent = (breakdown.principal_fiat_amount + totalDeductions).toFixed(2);
        }
      } catch (error) {
        log.error("[fiat_bridge] fee preview failed:", error);
      }
    };

    const onRuntimeUpdated = (/** @type {Event} */ event) => {
      runtime = /** @type {CustomEvent<SidecarRuntimeStats>} */ (event).detail;
      applyRuntimeToFeePanel(panel, runtime);
      void refresh();
    };
    window.addEventListener(SIDECAR_RUNTIME_EVENT, onRuntimeUpdated);

    volumeInput.addEventListener("input", () => void refresh());
    window.updateInvoicePreview = refresh;
    void refresh();

    return () => {
      window.removeEventListener(
        SIDECAR_RUNTIME_EVENT,
        /** @type {EventListener} */ (onRuntimeUpdated),
      );
    };
  },
};
