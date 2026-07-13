import { dispatchToModule } from "../../sidecar-api.js";
import { escapeHtml } from "../../dom-security.js";
import { formatCount, formatFiatFromShannons, formatShannons } from "../../stats-ui.js";
import { syncVaultIdentity, vaultState } from "./state.js";

/** @param {string | null | undefined} memberId @returns {string} */
function shortMemberId(memberId) {
  if (!memberId) return "Unknown";
  if (memberId.length <= 16) return memberId;
  return `${memberId.slice(0, 8)}…${memberId.slice(-8)}`;
}

/** @param {number | null | undefined} unixSeconds @returns {string} */
function formatTimestamp(unixSeconds) {
  if (!unixSeconds) return "—";
  return new Date(unixSeconds * 1000).toLocaleString();
}

export async function fetchVaultContributors(groupName = vaultState.groupName) {
  await syncVaultIdentity(groupName);
  return dispatchToModule("dicoba", "list_vault_contributors", {
    group_name: groupName,
    vault_id: vaultState.vaultId,
  });
}

/**
 * @param {{ groupName: string, contributors?: Array<Record<string, unknown>> }} params
 * @returns {string}
 */
export function renderVaultContributorsPanel({ groupName, contributors = [] }) {
  const rows = contributors.length
    ? contributors
        .map(
          (entry) => `
          <tr>
            <td><code>${escapeHtml(shortMemberId(String(entry.member_id ?? entry.memberId ?? "")))}</code></td>
            <td>${formatCount(Number(entry.contribution_count ?? entry.contributionCount ?? 0), { label: "streams" })}</td>
            <td>${formatShannons(Number(entry.total_shannons ?? entry.totalShannons ?? 0))}</td>
            <td>${formatFiatFromShannons(
              Number(entry.total_shannons ?? entry.totalShannons ?? 0),
              vaultState.conversionRate,
            )}</td>
            <td>${formatTimestamp(Number(entry.last_timestamp ?? entry.lastTimestamp ?? 0))}</td>
          </tr>
        `,
        )
        .join("")
    : `
      <tr>
        <td colspan="5" class="contributor-empty">No recorded contributors for this vault yet.</td>
      </tr>
    `;

  return `
    <section class="contributor-panel" data-vault-contributors>
      <div class="contributor-panel-header">
        <div>
          <h2>Contributing Members</h2>
          <p>Members who have streamed micro-contributions into <strong>${escapeHtml(groupName)}</strong>.</p>
        </div>
        <button type="button" class="refresh-btn refresh-btn-inline" data-action="refresh-vault-contributors">Refresh</button>
      </div>
      <div class="contributor-table-wrap">
        <table class="contributor-table">
          <thead>
            <tr>
              <th>Member ID</th>
              <th>Streams</th>
              <th>Total Shannons</th>
              <th>Total Fiat</th>
              <th>Last Contribution</th>
            </tr>
          </thead>
          <tbody>${rows}</tbody>
        </table>
      </div>
    </section>
  `;
}

/** @param {HTMLElement} root @param {{ onGroupNameChange?: () => void }} [options] */
export async function mountVaultContributors(root, { onGroupNameChange } = {}) {
  const host = root.querySelector("[data-vault-contributors-host]");
  if (!host) return async () => {};

  const paint = async () => {
    const groupInput = /** @type {HTMLInputElement | null} */ (
      root.querySelector("[data-dicoba-vault-name]")
    );
    const groupName = groupInput?.value?.trim() || vaultState.groupName;
    try {
      const result = /** @type {Record<string, unknown>} */ (
        await fetchVaultContributors(groupName)
      );
      host.innerHTML = renderVaultContributorsPanel({
        groupName: String(result.group_name ?? result.groupName ?? groupName),
        contributors: /** @type {Array<Record<string, unknown>>} */ (result.contributors ?? []),
      });
      host
        .querySelector('[data-action="refresh-vault-contributors"]')
        ?.addEventListener("click", () => {
          void paint();
        });
    } catch (error) {
      host.innerHTML = `
        <section class="contributor-panel" data-vault-contributors>
          <div class="contributor-panel-header">
            <div>
              <h2>Contributing Members</h2>
              <p class="contributor-error">Unable to load contributors: ${escapeHtml(error)}</p>
            </div>
          </div>
        </section>
      `;
    }
  };

  const vaultInput = root.querySelector("[data-dicoba-vault-name]");
  vaultInput?.addEventListener("change", () => {
    void paint();
    onGroupNameChange?.();
  });

  await paint();
  return paint;
}
