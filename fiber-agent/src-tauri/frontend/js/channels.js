import { createLogger } from "../dashboard/logger.js";
import { safeUserMessage, escapeHtml } from "./dom-security.js";
import { parseAtomicInt, formatShannons } from "../dashboard/money.js";
import { hasTauri, getFnnAddress } from "./sidecar-api.js";
import {
  listMeshChannels,
  listMfaDiscoverableAgents,
  openMeshChannel,
  closeMeshChannel,
} from "./sidecar-api.js";

const log = createLogger("channels");
const SHANNONS_PER_CKB = 100_000_000n;
const DEFAULT_FUND_CKB = "500";

/**
 * @param {HTMLElement} root
 * @param {string} message
 * @param {"info"|"ok"|"err"} [tone]
 */
function setStatus(root, message, tone = "info") {
  const el = root.querySelector("[data-channels-status]");
  if (!(el instanceof HTMLElement)) return;
  el.textContent = message;
  el.dataset.tone = tone;
}

/**
 * @param {bigint} ckb
 * @returns {bigint}
 */
function ckbToShannons(ckb) {
  return ckb * SHANNONS_PER_CKB;
}

/**
 * @param {bigint} shannons
 * @returns {string}
 */
function shannonsToCkbLabel(shannons) {
  const whole = shannons / SHANNONS_PER_CKB;
  const frac = shannons % SHANNONS_PER_CKB;
  if (frac === 0n) return whole.toString();
  const fracStr = frac.toString().padStart(8, "0").replace(/0+$/, "");
  return `${whole}.${fracStr}`;
}

/**
 * @param {HTMLElement} root
 * @returns {bigint}
 */
function readAmountCkb(root) {
  const input = root.querySelector("[data-channels-amount]");
  const raw = input instanceof HTMLInputElement ? input.value.trim() : DEFAULT_FUND_CKB;
  return parseAtomicInt(raw, "CKB");
}

/**
 * @param {HTMLElement} root
 * @param {Array<{
 *   agentId: number,
 *   online: boolean,
 *   fnnPubkeyHex?: string | null,
 *   peerConnectAddress?: string | null,
 *   isSelf: boolean,
 * }>} agents
 */
function paintAgents(root, agents) {
  const select = root.querySelector("[data-channels-peer]");
  if (!(select instanceof HTMLSelectElement)) return;
  const peers = agents.filter((a) => !a.isSelf);
  const previous = select.value;
  select.innerHTML = peers.length
    ? peers
        .map((a) => {
          const hasPk = Boolean(a.fnnPubkeyHex);
          const addr = a.peerConnectAddress ?? "";
          const loopback = /127\.0\.0\.1|\/ip4\/0\.0\.0\.0/.test(addr);
          const label = `FA-${a.agentId}${a.online ? "" : " (stale)"}${
            !hasPk
              ? " · awaiting Fiber pubkey"
              : !addr
                ? " · no P2P addr"
                : loopback
                  ? " · localhost-only (set FIBER_ANNOUNCE_ADDR)"
                  : ""
          }`;
          const value = hasPk ? a.fnnPubkeyHex ?? "" : "";
          return `<option value="${escapeHtml(value)}" data-address="${escapeHtml(
            addr,
          )}" data-agent="${a.agentId}" ${hasPk ? "" : "disabled"}>${escapeHtml(label)}</option>`;
        })
        .join("")
    : `<option value="">No other FAs online on MFA yet</option>`;
  if (previous && [...select.options].some((o) => o.value === previous)) {
    select.value = previous;
  }
}

/**
 * @param {HTMLElement} root
 * @param {Array<{
 *   peerId: number,
 *   peerPubkey?: string | null,
 *   channelId?: string | null,
 *   isActive: boolean,
 *   stateName?: string,
 *   localBalanceShannons: number,
 *   remoteBalanceShannons: number,
 * }>} channels
 */
function paintChannels(root, channels) {
  const body = root.querySelector("[data-channels-list]");
  if (!body) return;
  if (!channels.length) {
    body.innerHTML = `<p class="channels-empty">No Fiber channels yet. Fund your CKB testnet L1 lock (Funding tab / faucet), pick an MFA peer, then Open channel — only <code>ChannelReady</code> survives Refresh.</p>`;
    return;
  }
  body.innerHTML = `
    <table class="channels-table">
      <thead>
        <tr>
          <th>Peer</th>
          <th>Status</th>
          <th>Local</th>
          <th>Remote</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        ${channels
          .map((ch) => {
            const peer =
              ch.peerId > 0
                ? `FA-${ch.peerId}`
                : ch.peerPubkey
                  ? `${ch.peerPubkey.slice(0, 10)}…`
                  : "—";
            const status = ch.stateName || (ch.isActive ? "ChannelReady" : "Inactive");
            const channelId = ch.channelId ?? "";
            const peerPubkey = ch.peerPubkey ?? "";
            return `
              <tr>
                <td>
                  <div class="channels-peer">${escapeHtml(peer)}</div>
                  <div class="channels-id mono">${escapeHtml(
                    channelId ? `${channelId.slice(0, 18)}…` : "pending",
                  )}</div>
                </td>
                <td><span class="channels-pill" data-active="${ch.isActive}">${escapeHtml(status)}</span></td>
                <td>${escapeHtml(formatShannons(ch.localBalanceShannons, { suffix: false }))}</td>
                <td>${escapeHtml(formatShannons(ch.remoteBalanceShannons, { suffix: false }))}</td>
                <td>
                  <button type="button" class="ghost-btn" data-action="close-channel"
                    data-channel-id="${escapeHtml(channelId)}"
                    data-peer-pubkey="${escapeHtml(peerPubkey)}"
                    ${ch.isActive ? "" : "disabled"}>Close</button>
                </td>
              </tr>`;
          })
          .join("")}
      </tbody>
    </table>`;
}

/**
 * @param {HTMLElement} root
 */
export async function refreshChannelsPanel(root) {
  if (!hasTauri()) {
    setStatus(root, "Desktop app unavailable", "err");
    return;
  }
  setStatus(root, "Loading MFA peers and channels…", "info");
  try {
    const [discovery, channels, funding] = await Promise.all([
      listMfaDiscoverableAgents(),
      listMeshChannels(),
      getFnnAddress().catch(() => null),
    ]);
    paintAgents(root, discovery.agents);
    paintChannels(root, channels);
    const l1Hint = root.querySelector("[data-channels-l1]");
    if (l1Hint instanceof HTMLElement) {
      if (funding && funding.network === "testnet") {
        const bal = BigInt(funding.l1BalanceShannons || 0);
        l1Hint.textContent =
          bal === 0n
            ? `CKB testnet L1 funding lock: 0 CKB — channels will not stick until you faucet ${funding.address || "your ckt1 address"}`
            : `CKB testnet L1 funding lock: ${funding.l1BalanceCkb} CKB (${funding.address || "ckt1…"})`;
        l1Hint.dataset.tone = bal === 0n ? "err" : "ok";
      } else if (funding) {
        l1Hint.textContent = `FNN network: ${funding.network || "unknown"} · L1 ${funding.l1BalanceCkb || "?"} CKB`;
        l1Hint.dataset.tone = "info";
      } else {
        l1Hint.textContent = "Could not read FNN funding lock (is FNN on testnet running?)";
        l1Hint.dataset.tone = "err";
      }
    }
    const amountInput = root.querySelector("[data-channels-amount]");
    if (amountInput instanceof HTMLInputElement && !amountInput.dataset.touched) {
      const defaultCkb = BigInt(discovery.defaultFundingShannons) / SHANNONS_PER_CKB;
      amountInput.value = defaultCkb > 0n ? defaultCkb.toString() : DEFAULT_FUND_CKB;
    }
    const peerCount = discovery.agents.filter((a) => !a.isSelf && a.fnnPubkeyHex).length;
    const active = channels.filter((c) => c.isActive).length;
    const pending = channels.filter((c) => !c.isActive).length;
    const l1Zero =
      funding && BigInt(funding.l1BalanceShannons || 0) === 0n
        ? " · fund L1 on testnet or opens will disappear"
        : "";
    setStatus(
      root,
      `${peerCount} discoverable peer${peerCount === 1 ? "" : "s"} on ${discovery.mfaHost} · ${active} ready / ${pending} pending / ${channels.length} total${l1Zero}`,
      funding && BigInt(funding.l1BalanceShannons || 0) === 0n ? "err" : "ok",
    );
  } catch (error) {
    log.error("refresh channels failed", error);
    setStatus(root, safeUserMessage(error, "Could not load peers/channels"), "err");
  }
}

/**
 * @param {HTMLElement} root
 */
async function openSelectedChannel(root) {
  const select = root.querySelector("[data-channels-peer]");
  if (!(select instanceof HTMLSelectElement) || !select.value) {
    setStatus(root, "Select an MFA peer with a Fiber pubkey", "err");
    return;
  }
  const option = select.selectedOptions[0];
  const peerAddress = option?.dataset.address || undefined;
  const amountCkb = readAmountCkb(root);
  if (amountCkb < 1n) {
    setStatus(root, "Funding amount must be at least 1 CKB", "err");
    return;
  }
  const amountShannons = ckbToShannons(amountCkb);
  setStatus(
    root,
    `Opening channel (${shannonsToCkbLabel(amountShannons)} CKB)…`,
    "info",
  );
  try {
    const message = await openMeshChannel({
      peerPubkey: select.value,
      amountShannons: Number(amountShannons),
      peerAddress: peerAddress || null,
    });
    setStatus(root, safeUserMessage(message, "Channel open requested"), "ok");
    await refreshChannelsPanel(root);
  } catch (error) {
    log.error("open_mesh_channel failed", error);
    setStatus(root, safeUserMessage(error, "Could not open channel"), "err");
  }
}

/**
 * @param {HTMLElement} root
 * @param {HTMLElement} btn
 */
async function closeChannelFromRow(root, btn) {
  const channelId = btn.dataset.channelId || "";
  const peerPubkey = btn.dataset.peerPubkey || "";
  if (!channelId && !peerPubkey) {
    setStatus(root, "Missing channel id", "err");
    return;
  }
  setStatus(root, "Closing channel…", "info");
  try {
    const message = await closeMeshChannel({
      channelId: channelId || null,
      peerPubkey: peerPubkey || null,
      force: false,
    });
    setStatus(root, safeUserMessage(message, "Channel close requested"), "ok");
    await refreshChannelsPanel(root);
  } catch (error) {
    log.error("close_mesh_channel failed", error);
    setStatus(root, safeUserMessage(error, "Could not close channel"), "err");
  }
}

export function channelsPanelMarkup() {
  return `
    <section class="channels-panel funding-onboard">
      <p class="funding-kicker">Mesh · MFA discovery</p>
      <div class="funding-address-bar">
        <h2 class="funding-address-title">Channels</h2>
        <p class="panel-lead">Open or close real Fiber channels on <strong>CKB testnet</strong> with other Fiber Agents on MFA.</p>
        <p class="funding-status" data-channels-status data-tone="info">Loading…</p>
        <p class="panel-hint" data-channels-l1 data-tone="info">Checking testnet L1 funding lock…</p>
      </div>

      <div class="channels-open-card">
        <label class="channels-label" for="channels-peer">Peer (from MFA)</label>
        <select id="channels-peer" class="channels-select" data-channels-peer>
          <option value="">Loading peers…</option>
        </select>

        <label class="channels-label" for="channels-amount">Funding (CKB)</label>
        <input id="channels-amount" class="channels-input" type="text" inputmode="numeric"
          data-channels-amount value="${DEFAULT_FUND_CKB}" />

        <p class="panel-hint">
          Opens use on-chain testnet CKB from your Funding lock. With 0 L1 CKB, Fiber briefly
          negotiates then drops the channel (Refresh shows 0). Cross-machine also needs a
          reachable P2P addr — set <code>FIBER_ANNOUNCE_ADDR=/ip4/&lt;LAN-IP&gt;/tcp/8228</code> if needed.
        </p>

        <div class="funding-address-actions">
          <button type="button" class="primary-btn" data-action="open-channel">Open channel</button>
          <button type="button" class="ghost-btn" data-action="refresh-channels">Refresh</button>
        </div>
      </div>

      <div class="channels-list-wrap">
        <h3 class="channels-list-title">Your channels</h3>
        <div data-channels-list></div>
      </div>
    </section>
  `;
}

/**
 * @param {HTMLElement} root
 */
export async function mountChannelsPanel(root) {
  const amountInput = root.querySelector("[data-channels-amount]");
  if (amountInput instanceof HTMLInputElement) {
    amountInput.addEventListener("input", () => {
      amountInput.dataset.touched = "1";
    });
  }

  root.addEventListener("click", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement)) return;
    const action = target.closest("[data-action]")?.getAttribute("data-action");
    if (action === "refresh-channels") {
      void refreshChannelsPanel(root);
    } else if (action === "open-channel") {
      void openSelectedChannel(root);
    } else if (action === "close-channel") {
      const btn = target.closest("[data-action=close-channel]");
      if (btn instanceof HTMLElement) void closeChannelFromRow(root, btn);
    }
  });

  await refreshChannelsPanel(root);
}
