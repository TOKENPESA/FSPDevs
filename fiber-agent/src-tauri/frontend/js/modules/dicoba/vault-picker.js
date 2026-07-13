import { dispatchToModule } from "../../sidecar-api.js";
import { vaultState } from "./state.js";

export async function fetchMemberVaults() {
  await dispatchToModule("dicoba", "get_vault_context", {
    group_name: vaultState.groupName,
  }).then((context) => {
    const ctx = /** @type {Record<string, string>} */ (context);
    vaultState.localMemberId =
      ctx.local_member_id ?? vaultState.localMemberId;
  });

  const result = /** @type {Record<string, unknown>} */ (
    await dispatchToModule("dicoba", "list_member_vaults", {
      member_id: vaultState.localMemberId,
    })
  );

  const vaults = /** @type {Array<Record<string, unknown>>} */ (result.vaults ?? []);
  const names = new Set(vaults.map((vault) => String(vault.group_name ?? vault.groupName)));

  if (vaultState.groupName && !names.has(vaultState.groupName)) {
    vaults.unshift({
      group_name: vaultState.groupName,
      vault_id: vaultState.vaultId,
      contribution_count: 0,
      total_shannons: 0,
      last_timestamp: 0,
      is_default: true,
    });
  }

  return vaults;
}

/** @param {Record<string, unknown>} vault @returns {string} */
function vaultLabel(vault) {
  const name = String(vault.group_name ?? vault.groupName ?? "");
  const streams = Number(vault.contribution_count ?? vault.contributionCount ?? 0);
  return streams > 0
    ? `${name} (${streams} contribution${streams === 1 ? "" : "s"})`
    : `${name} (no contributions yet)`;
}

/** @param {Array<Record<string, unknown>>} vaults @param {string} [selectedName] @returns {string} */
export function renderVaultSelectOptions(vaults, selectedName = vaultState.groupName) {
  const options = vaults
    .map((vault) => {
      const name = String(vault.group_name ?? vault.groupName);
      const label = vaultLabel(vault);
      return `<option value="${name}"${name === selectedName ? " selected" : ""}>${label}</option>`;
    })
    .join("");

  return `
    ${options}
    <option value="__custom__"${selectedName && !vaults.some((vault) => String(vault.group_name ?? vault.groupName) === selectedName) ? " selected" : ""}>Other vault…</option>
  `;
}

/** @param {HTMLElement} root @param {{ onVaultChange?: () => void | Promise<void> }} [options] */
export async function mountLoanVaultPicker(root, { onVaultChange } = {}) {
  const select = /** @type {HTMLSelectElement | null} */ (
    root.querySelector("[data-dicoba-loan-vault]")
  );
  const customInput = /** @type {HTMLInputElement | null} */ (
    root.querySelector("[data-dicoba-loan-vault-custom]")
  );
  if (!select) return;

  const paint = async () => {
    const vaults = await fetchMemberVaults();
    const selected = select.value || vaultState.groupName;
    select.innerHTML = renderVaultSelectOptions(vaults, selected);

    const useCustom = select.value === "__custom__";
    if (customInput) {
      customInput.hidden = !useCustom;
      customInput.style.display = useCustom ? "" : "none";
      if (useCustom && !customInput.value) {
        customInput.value = vaultState.groupName;
      }
    }
  };

  const applySelection = async () => {
    const useCustom = select.value === "__custom__";
    if (customInput) {
      customInput.hidden = !useCustom;
      customInput.style.display = useCustom ? "" : "none";
    }
    await onVaultChange?.();
  };

  select.addEventListener("change", () => {
    void applySelection();
  });
  customInput?.addEventListener("change", () => {
    void onVaultChange?.();
  });
  customInput?.addEventListener("blur", () => {
    void onVaultChange?.();
  });

  await paint();
  return paint;
}
