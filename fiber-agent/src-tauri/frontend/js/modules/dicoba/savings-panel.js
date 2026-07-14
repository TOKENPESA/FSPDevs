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
  title: "Group savings",
  navLabel: "Savings",
  navIcon: "savings",
  badge: "Savings",
  navDescription: "Add money to your group savings",
  render() {
    return `
      <div class="module-workspace-inner" data-panel="dicoba-savings">
        <div class="workspace-card">
          <div class="input-group">
            <label>Group vault name</label>
            <input type="text" data-dicoba-vault-name value="${vaultState.groupName}">
          </div>
          <div class="input-group">
            <label>Amount (TZS)</label>
            <input type="number" data-dicoba-contribution value="2500">
          </div>
          <button type="button" class="primary-btn" data-action="dicoba-contribute">Add contribution</button>
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
          log.innerHTML = "Sending your contribution…";
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
            log.innerHTML = `✅ <strong>Contribution sent</strong><br>Vault: ${escapeHtml(vaultState.groupName)}<br>Reference: ${escapeHtml(String(receipt.transaction_id))}<br>Member: ${escapeHtml(String(receipt.member_id))}<br>Amount: ${escapeHtml(String(amount))} TZS<br>Time: ${escapeHtml(new Date(Number(receipt.timestamp) * 1000).toLocaleTimeString())}`;
          }
          await syncVaultTransparency();
          await repaintStats();
          await repaintContributors();
          refreshLoanPanel?.();
        } catch (error) {
          if (log instanceof HTMLElement) {
            log.innerHTML = `❌ <strong>Couldn't complete contribution:</strong> ${escapeHtml(errorMessage(error))}`;
          }
        }
      });
  },
};
