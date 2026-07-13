import {
  COMM_TTL_MS,
  MFA_HUB,
  PATH_STYLES,
  PAYMENT_SETTLE_DISPLAY_MS,
} from "../config.js";
import { formatShannons } from "../format.js";
import { $ } from "../dom.js";
import {
  resolveNodeBalances,
  state,
  isCommLive,
} from "../state.js";
import { gridDim, meshPeerLinks } from "../topology.js";

const faTooltip = $("fa-tooltip");
const canvas = document.getElementById("grid");
const canvasWrap = canvas?.closest(".canvas-wrap") ?? null;

/** @param {number} id @returns {{ label: string, cls: string }} */
export function nodeStatus(id) {
  if (state.dead.has(id)) {
    return { label: "OFFLINE / PARTITIONED", cls: "offline" };
  }
  const recv = state.comm.received.get(id);
  if (recv && Date.now() - recv.at < PAYMENT_SETTLE_DISPLAY_MS) {
    return {
      label: `RECEIVED +${formatShannons(recv.amount)} from FA-${recv.from}`,
      cls: "healed",
    };
  }
  const transfer = state.paymentTransfer;
  if (transfer?.destination === id && transfer.phase === "traveling") {
    return { label: "INCOMING PAYMENT…", cls: "healed" };
  }
  const bal = resolveNodeBalances(id);
  if (bal) {
    return {
      label: `BALANCE · out ${formatShannons(bal.outbound)} · in ${formatShannons(bal.inbound)}`,
      cls: "healed",
    };
  }
  const comm = state.comm.nodes.get(id);
  if (comm && Date.now() - comm.at <= COMM_TTL_MS) {
    const n = comm.neighbors?.length ?? 0;
    return { label: `LIVE · ${n} mesh peer(s) · MFA linked`, cls: "healed" };
  }
  const rq = state.liquidity.byNode.get(id);
  if (rq && Date.now() - rq.at < 120_000) {
    /** @type {Record<string, string>} */
    const labels = {
      funded: "HUB FUNDED (LIQUIDITY OK)",
      faucet: "NEEDS TESTNET FAUCET",
      failed: "LIQUIDITY FAILED",
      engaged: "LIQUIDITY IN FLIGHT",
      started: "LIQUIDITY STARTING",
    };
    return { label: labels[rq.status] ?? "LIQUIDITY", cls: rq.status === "funded" ? "healed" : "offline" };
  }
  return { label: "ACTIVE (FNN SYNCED)", cls: "active" };
}

/** @param {number | null} id @param {number} clientX @param {number} clientY */
export function updateTooltip(id, clientX, clientY) {
  if (!faTooltip) return;
  if (!id) {
    faTooltip.classList.remove("visible");
    faTooltip.setAttribute("aria-hidden", "true");
    return;
  }

  const status = nodeStatus(id);
  const links = meshPeerLinks(id);
  const livePeers = links.filter((l) => !state.dead.has(l.peer));
  const idx = id - 1;
  const dim = gridDim();
  const col = idx % dim;
  const row = Math.floor(idx / dim);

  const linkRows = links.map(({ peer, kind }) => {
    const st = PATH_STYLES[/** @type {keyof typeof PATH_STYLES} */ (kind)];
    const live = !state.dead.has(peer);
    const comm = isCommLive(peer);
    const tag = comm ? " comm" : "";
    return `<div class="fa-row"><span style="color:${st.color}">●</span> ${st.label}: <span>FA-${peer}${live ? "" : " (down)"}${tag}</span></div>`;
  }).join("");

  const resolved = resolveNodeBalances(id);
  const hasBalances = resolved != null;
  const balanceRows = hasBalances
    ? `<div class="fa-balances">
        <div class="fa-row fa-balance-out">Outbound: <span>${formatShannons(resolved.outbound)}</span></div>
        <div class="fa-row fa-balance-in">Inbound: <span>${formatShannons(resolved.inbound)}</span></div>
      </div>`
    : `<div class="fa-balances">
        <div class="fa-row">Asset balance: <span>awaiting heartbeat or payment</span></div>
      </div>`;

  const recv = state.comm.received.get(id);
  const recvRow = recv
    ? `<div class="fa-row fa-balance-in">Last received: <span>+${formatShannons(recv.amount)} from FA-${recv.from}</span></div>`
    : "";

  const sent = state.comm.sent.get(id);
  const sentRow = sent
    ? `<div class="fa-row fa-balance-out">Last sent: <span>−${formatShannons(sent.totalDebit ?? sent.amount + (sent.fee ?? 0))} to FA-${sent.to}</span></div>`
    : "";

  faTooltip.innerHTML = `
    <div class="fa-id">FA-${id}</div>
    <div class="fa-status ${status.cls}">${status.label}</div>
    <div class="fa-row">Grid: <span>[${col}, ${row}]</span></div>
    <div class="fa-row">Mesh peers: <span>${livePeers.length}/3 live</span></div>
    ${balanceRows}
    ${recvRow}
    ${sentRow}
    ${linkRows}
    <div class="fa-row"><span style="color:${PATH_STYLES.mfa.color}">●</span> ${PATH_STYLES.mfa.label}: <span>127.0.0.1:1025</span></div>
  `;

  if (!canvasWrap) return;
  const wrapRect = canvasWrap.getBoundingClientRect();
  let left = clientX - wrapRect.left + 14;
  let top = clientY - wrapRect.top + 14;
  faTooltip.classList.add("visible");
  faTooltip.setAttribute("aria-hidden", "false");

  const tipRect = faTooltip.getBoundingClientRect();
  if (left + tipRect.width > wrapRect.width - 8) {
    left = clientX - wrapRect.left - tipRect.width - 14;
  }
  if (top + tipRect.height > wrapRect.height - 8) {
    top = clientY - wrapRect.top - tipRect.height - 14;
  }
  faTooltip.style.left = `${Math.max(8, left)}px`;
  faTooltip.style.top = `${Math.max(8, top)}px`;
}

export function hideTooltip() {
  if (!faTooltip) return;
  faTooltip.classList.remove("visible");
  faTooltip.setAttribute("aria-hidden", "true");
}
