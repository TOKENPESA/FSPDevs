import { dispatchToModule } from "../../sidecar-api.js";
import { escapeHtml } from "../../dom-security.js";
import { errorMessage } from "../../../packages/fsp-ui-types/errors.js";
import { fiatMinorToShannons, parseFiatMinorUnits } from "../../../dashboard/money.js";
import {
  mountDicobaStats,
  renderLoanStats,
} from "./module-stats.js";
import {
  buildVaultConfig,
  getSelectedVaultName,
  syncVaultIdentity,
  syncVaultStateFromDom,
  syncVaultTransparency,
  vaultState,
} from "./state.js";
import { mountLoanVaultPicker } from "./vault-picker.js";
import { showOobFallbackInHost } from "../../oob-fallback.js";
import { getSidecarStats, resolveDicobaMemberId } from "../../sidecar-api.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarPanel} SidecarPanel */

/** @type {SidecarPanel & {_repaintStats?: () => Promise<void>}} */
export const loanPanel = {
  id: "dicoba-loan",
  title: "Smart Loan & Guarantor Staking",
  navLabel: "Smart Loans",
  navIcon: "loans",
  badge: "dicoba / request_loan",
  navDescription:
    "Request algorithmic loans with decentralized guarantor staking",
  render() {
    return `
      <div class="module-workspace-inner" data-panel="dicoba-loan">
        <div class="workspace-card">
          <div class="input-group">
            <label>Target JunguKuu Vault</label>
            <select data-dicoba-loan-vault aria-label="Select vault for loan request">
              <option value="${vaultState.groupName}">${vaultState.groupName}</option>
            </select>
            <input
              type="text"
              data-dicoba-loan-vault-custom
              hidden
              placeholder="Enter vault name"
              aria-label="Custom vault name"
            >
          </div>
          <div class="input-group">
            <label>Requested Loan Amount (TZS)</label>
            <input type="number" data-dicoba-loan-amount value="50000" min="1000">
          </div>
          <div class="input-group">
            <label>Guarantor Member ID (receiving agent's DiCoBa ID)</label>
            <input type="text" data-dicoba-guarantor placeholder="Auto-filled from mesh peer when available">
          </div>
          <button type="button" class="primary-btn btn-loan" data-action="dicoba-request-loan">Initiate Smart Loan Contract</button>
          <button type="button" class="hero-btn" data-action="dicoba-oob-guarantor" hidden>
            Generate OOB QR for Guarantor
          </button>
          <div class="receipt-log loan" data-dicoba-loan-log style="display:none;"></div>
          <div class="receipt-log oob" data-dicoba-oob-log style="display:none;"></div>
        </div>
      </div>
    `;
  },
  /** @param {HTMLElement} root */
  async mount(root) {
    const panel = root.querySelector('[data-panel="dicoba-loan"]');
    if (!(panel instanceof HTMLElement)) return;

    const log = panel.querySelector("[data-dicoba-loan-log]");
    const oobLog = panel.querySelector("[data-dicoba-oob-log]");
    const oobBtn = panel.querySelector("[data-action='dicoba-oob-guarantor']");
    /** @type {{ guarantorId: string, groupName: string, principalShannons: number, stagedPrincipal: number } | null} */
    let lastLoanContext = null;

    const refreshVaultContext = async () => {
      syncVaultStateFromDom(root, "loan");
      const groupName = getSelectedVaultName(root, "loan");
      await syncVaultIdentity(groupName);
      await syncVaultTransparency(groupName);
      await this._repaintStats?.();
    };

    this._repaintStats =
      (await mountDicobaStats(
        root,
        '[data-panel="dicoba-loan"]',
        renderLoanStats,
      )) ?? (async () => {});

    await mountLoanVaultPicker(root, {
      onVaultChange: () => refreshVaultContext(),
    });
    await refreshVaultContext();

    const guarantorInput = panel.querySelector("[data-dicoba-guarantor]");
    try {
      const stats = await getSidecarStats();
      const peerId = stats?.meshPeerAgentId;
      if (guarantorInput instanceof HTMLInputElement && peerId && !guarantorInput.value.trim()) {
        guarantorInput.value =
          stats?.meshPeerDicobaMemberId ??
          (await resolveDicobaMemberId(peerId));
      }
    } catch {
      /* optional prefill */
    }

    panel
      .querySelector('[data-action="dicoba-request-loan"]')
      ?.addEventListener("click", async () => {
        const amountInput = panel.querySelector("[data-dicoba-loan-amount]");
        const guarantorField = panel.querySelector("[data-dicoba-guarantor]");
        if (!(amountInput instanceof HTMLInputElement) || !(guarantorField instanceof HTMLInputElement)) {
          return;
        }

        const loanFiatMinor = parseFiatMinorUnits(amountInput.value, "fiat");
        const loanFiat = Number(loanFiatMinor);
        const guarantorId = guarantorField.value.trim();
        syncVaultStateFromDom(root, "loan");
        const groupName = getSelectedVaultName(root, "loan");
        await syncVaultIdentity(groupName);

        if (!groupName) {
          if (log instanceof HTMLElement) {
            log.style.display = "block";
            log.innerHTML =
              "❌ <strong>Error:</strong> Select or enter a target vault.";
          }
          return;
        }

        if (!guarantorId) {
          if (log instanceof HTMLElement) {
            log.style.display = "block";
            log.innerHTML =
              "❌ <strong>Error:</strong> A valid Guarantor ID is required to stake shares.";
          }
          return;
        }

        if (log instanceof HTMLElement) {
          log.style.display = "block";
          log.innerHTML = `⚡ Drafting loan contract for <strong>${escapeHtml(groupName)}</strong>...`;
        }

        try {
          const receipt = /** @type {Record<string, unknown>} */ (
            await dispatchToModule("dicoba", "request_loan", {
            principal_shannons: Number(
              fiatMinorToShannons(loanFiatMinor, vaultState.conversionRate),
            ),
            guarantor_id: guarantorId,
            group_name: groupName,
            vault_config: buildVaultConfig(groupName),
            })
          );
          if (log instanceof HTMLElement) {
            log.innerHTML = `✅ <strong>Loan Contract Staged!</strong><br>Vault: ${escapeHtml(groupName)}<br>Status: ${escapeHtml(String(receipt.status ?? ""))}<br>${escapeHtml(String(receipt.message ?? ""))}<br>Awaiting Cryptographic Signature from Guarantor ID: ${escapeHtml(guarantorId.substring(0, 8))}...`;
          }

          const principalShannons = Number(
            fiatMinorToShannons(loanFiatMinor, vaultState.conversionRate),
          );
          lastLoanContext = {
            guarantorId,
            groupName,
            principalShannons,
            stagedPrincipal: Number(receipt.staged_principal ?? principalShannons),
          };

          if (oobBtn instanceof HTMLElement) oobBtn.hidden = false;

          try {
            const stats = await getSidecarStats();
            if (!stats?.mfaControlConnected && lastLoanContext && oobLog instanceof HTMLElement) {
              await showOobFallbackInHost(oobLog, {
                targetModule: "dicoba",
                targetAgent: stats?.meshPeerAgentId || 12,
                method: "request_guarantor_signature",
                payload: {
                  loan_id: `loan-${groupName.replace(/\s+/g, "-").toLowerCase()}`,
                  guarantor_member_id: guarantorId,
                  principal_shannons: lastLoanContext.stagedPrincipal,
                },
              });
            }
          } catch {
            /* stats optional for OOB auto-prompt */
          }
        } catch (error) {
          if (log instanceof HTMLElement) {
            log.innerHTML = `❌ <strong>Contract Rejected:</strong> ${escapeHtml(errorMessage(error))}`;
          }
        }
      });

    oobBtn?.addEventListener("click", async () => {
      if (!lastLoanContext || !(oobLog instanceof HTMLElement)) return;
      const stats = await getSidecarStats().catch(() => null);
      await showOobFallbackInHost(oobLog, {
        targetModule: "dicoba",
        targetAgent: stats?.meshPeerAgentId || 12,
        method: "request_guarantor_signature",
        payload: {
          loan_id: `loan-${lastLoanContext.groupName.replace(/\s+/g, "-").toLowerCase()}`,
          guarantor_member_id: lastLoanContext.guarantorId,
          principal_shannons: lastLoanContext.stagedPrincipal,
        },
      });
    });
  },
  /** @param {HTMLElement} root */
  async refresh(root) {
    syncVaultStateFromDom(root, "loan");
    const groupName = getSelectedVaultName(root, "loan");
    await syncVaultTransparency(groupName);
    await this._repaintStats?.();
  },
};
