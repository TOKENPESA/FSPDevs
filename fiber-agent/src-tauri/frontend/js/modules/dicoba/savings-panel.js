import {
  mountDicobaStats,
  renderSavingsStats,
} from "./module-stats.js";
import { mountVaultContributors } from "./vault-contributors.js";
import { executeDicoContribution } from "../../sidecar-api.js";
import { escapeHtml } from "../../dom-security.js";
import { errorMessage } from "../../../packages/fsp-ui-types/errors.js";
import { parseFiatMinorUnits } from "../../../dashboard/money.js";
import {
  buildVaultConfig,
  syncVaultIdentity,
  syncVaultStateFromDom,
  syncVaultTransparency,
  vaultState,
} from "./state.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarPanel} SidecarPanel */
/** @typedef {import("../../../../../../dashboard/types.js").SidecarUiContext} SidecarUiContext */

/** @type {SidecarPanel} */
export const savingsPanel = {
  id: "dicoba-savings",
  title: "DICOBA Collective Savings",
  navLabel: "Collective Savings",
  navIcon: "savings",
  badge: "dicoba / stream_micro_contribution",
  navDescription:
    "Stream micro-contributions into approved JunguKuu group vaults",
  render() {
    return `
      <div class="module-workspace-inner" data-panel="dicoba-savings">
        <div class="workspace-card">
          <div class="input-group">
            <label>Target Group JunguKuu Vault Name</label>
            <input type="text" data-dicoba-vault-name value="${vaultState.groupName}">
          </div>
          <div class="input-group">
            <label>Contribution Amount (Fiat)</label>
            <input type="number" data-dicoba-contribution value="2500">
          </div>
          <button type="button" class="primary-btn" data-action="dicoba-contribute">Stream Micro-Contribution</button>
          <div class="receipt-log" data-dicoba-savings-log style="display:none;"></div>
        </div>
        <div data-vault-contributors-host></div>
      </div>
    `;
  },
  /** @param {HTMLElement} root @param {SidecarUiContext} [ctx] */
  async mount(root, ctx) {
    const { refreshLoanPanel } = ctx ?? { root };
    const panel = root.querySelector('[data-panel="dicoba-savings"]');
    if (!(panel instanceof HTMLElement)) return;

    const log = panel.querySelector("[data-dicoba-savings-log]");
    await syncVaultIdentity(vaultState.groupName);

    const repaintStats =
      (await mountDicobaStats(
        root,
        '[data-panel="dicoba-savings"]',
        renderSavingsStats,
      )) ?? (async () => {});

    const repaintContributors =
      (await mountVaultContributors(root)) ?? (async () => {});

    panel
      .querySelector('[data-action="dicoba-contribute"]')
      ?.addEventListener("click", async () => {
        const amountInput = panel.querySelector("[data-dicoba-contribution]");
        if (!(amountInput instanceof HTMLInputElement)) return;

        const amount = Number(parseFiatMinorUnits(amountInput.value, "fiat"));
        syncVaultStateFromDom(root);
        await syncVaultIdentity(vaultState.groupName);
        const vaultConfig = buildVaultConfig();

        if (log instanceof HTMLElement) {
          log.style.display = "block";
          log.innerHTML =
            "⚡ Initializing off-chain multi-hop micropayment stream...";
        }

        try {
          const receipt = /** @type {Record<string, string | number>} */ (
            await executeDicoContribution({
            vaultConfig: buildVaultConfig(vaultState.groupName),
            amountFiat: amount,
            shannonsConversionRate: vaultState.conversionRate,
            })
          );
          if (log instanceof HTMLElement) {
            log.innerHTML = `✅ <strong>Stream Succeeded</strong><br>Vault: ${escapeHtml(vaultState.groupName)}<br>Tx ID: ${escapeHtml(String(receipt.transaction_id))}<br>Member: ${escapeHtml(String(receipt.member_id))}<br>Value: ${escapeHtml(String(receipt.amount_shannons))} Shannons<br>Timestamp: ${escapeHtml(new Date(Number(receipt.timestamp) * 1000).toLocaleTimeString())}`;
          }
          await syncVaultTransparency();
          await repaintStats();
          await repaintContributors();
          refreshLoanPanel?.();
        } catch (error) {
          if (log instanceof HTMLElement) {
            log.innerHTML = `❌ <strong>Execution Failure:</strong> ${escapeHtml(errorMessage(error))}`;
          }
        }
      });
  },
};
