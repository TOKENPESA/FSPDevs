import { createLogger } from "../dashboard/logger.js";
import { parseAtomicInt } from "../dashboard/money.js";
import { escapeHtml, safeUserMessage } from "./dom-security.js";
import { getFnnAddress, hasTauri, openExternalUrl } from "./sidecar-api.js";

const log = createLogger("funding");

const FAUCET_URL = "https://faucet.nervos.org/";
/** JoyID passkey portal (testnet). */
const JOYID_TESTNET_URL = "https://testnet.joyid.dev";
/** CKB → shannons (1 CKB = 10^8 shannons). */
const SHANNONS_PER_CKB = 100_000_000n;
/** Default JoyID → sidecar funding amount (whole CKB). */
const DEFAULT_FUND_CKB = 100;
const APP_NAME = "FSP Sidecar";
/** Public HTTPS icon for JoyID dapp metadata (data: URLs are rejected by some builds). */
const APP_ICON = "https://cdn.jsdelivr.net/gh/nervosnetwork/neuron@master/packages/neuron-wallet/assets/icons/icon.png";

/** @type {string} */
let cachedAddress = "";
/** @type {Record<string, unknown> | null} */
let cachedLockScript = null;
/** @type {any} */
let activeJoySigner = null;
/** @type {string} */
let connectedWalletLabel = "";

/**
 * Lazy-load CCC + JoyID modules via the HTML import map (esm.sh fallback).
 * @returns {Promise<{ ccc: any, CkbSigner: any, connectorReady: boolean }>}
 */
async function loadCcc() {
  const [{ ccc }, joyMod, connector] = await Promise.all([
    import("@ckb-ccc/core"),
    import("@ckb-ccc/joy-id").catch(() => null),
    import("@ckb-ccc/connector").catch(() => null),
  ]);
  const CkbSigner =
    joyMod?.JoyId?.CkbSigner ??
    joyMod?.CkbSigner ??
    null;
  return { ccc, CkbSigner, connectorReady: Boolean(connector) };
}

/**
 * @param {HTMLElement} root
 * @param {string} message
 * @param {"info" | "ok" | "err"} [tone]
 */
function setStatus(root, message, tone = "info") {
  const el = root.querySelector("[data-funding-status]");
  if (!(el instanceof HTMLElement)) return;
  el.textContent = message;
  el.dataset.tone = tone;
}

/**
 * @param {HTMLElement} root
 * @param {{ connected: boolean, label?: string }} state
 */
function paintWalletChip(root, state) {
  const chip = root.querySelector("[data-funding-wallet]");
  if (!(chip instanceof HTMLElement)) return;
  chip.dataset.connected = state.connected ? "true" : "false";
  chip.textContent = state.connected
    ? state.label || "JoyID connected"
    : "Passkey not connected";
  const cta = root.querySelector("[data-action='funding-joyid']");
  if (cta instanceof HTMLButtonElement) {
    cta.textContent = state.connected ? "Send with JoyID" : "Continue with JoyID";
  }
}

/**
 * @param {HTMLElement} root
 * @param {{
 *   address: string,
 *   pubkey?: string,
 *   source?: string,
 *   network?: string,
 *   fundingLockScript?: Record<string, unknown>,
 *   l1BalanceCkb?: string,
 *   l1BalanceShannons?: number | string,
 *   l1BalanceSource?: string,
 * }} snapshot
 */
function paintAddress(root, snapshot) {
  cachedAddress = snapshot.address;
  cachedLockScript = snapshot.fundingLockScript ?? null;
  const addrEl = root.querySelector("[data-funding-address]");
  if (addrEl) addrEl.textContent = snapshot.address;
  const l1El = root.querySelector("[data-funding-l1-balance]");
  if (l1El) {
    const ckb = snapshot.l1BalanceCkb ?? "0";
    const src = String(snapshot.l1BalanceSource ?? "");
    if (src.startsWith("unavailable")) {
      l1El.textContent = "L1 funding lock: unavailable (check CKB RPC)";
    } else if (src === "simulate") {
      l1El.textContent = "L1 funding lock: n/a in demo mode";
    } else {
      l1El.textContent = `L1 funding lock: ${ckb} CKB`;
    }
  }
  const metaEl = root.querySelector("[data-funding-meta]");
  if (metaEl) {
    const network =
      snapshot.network === "testnet" || !snapshot.network
        ? "test coins"
        : String(snapshot.network);
    const source =
      snapshot.source === "simulated"
        ? "demo"
        : snapshot.source === "fnn"
          ? "live"
          : snapshot.source
            ? String(snapshot.source)
            : "live";
    metaEl.textContent = [network, source].filter(Boolean).join(" · ");
  }
}

/**
 * @param {HTMLElement} root
 * @returns {bigint}
 */
function readAmountCkb(root) {
  const amountInput = root.querySelector("[data-funding-amount]");
  const rawAmount =
    amountInput instanceof HTMLInputElement ? amountInput.value.trim() : String(DEFAULT_FUND_CKB);
  return parseAtomicInt(rawAmount, "CKB");
}

/**
 * @param {HTMLElement} root
 * @param {bigint} next
 */
function writeAmountCkb(root, next) {
  const amountInput = root.querySelector("[data-funding-amount]");
  if (!(amountInput instanceof HTMLInputElement)) return;
  const safe = next < 1n ? 1n : next;
  amountInput.value = safe.toString();
}

/**
 * @param {HTMLElement} root
 */
export async function refreshFnnAddress(root) {
  setStatus(root, "Loading your receive address…", "info");
  try {
    if (!hasTauri()) {
      throw new Error("Desktop app unavailable — open Fiber Agent to continue");
    }
    const snapshot = await getFnnAddress();
    paintAddress(root, {
      address: snapshot.address,
      pubkey: snapshot.pubkey,
      source: snapshot.source,
      network: snapshot.network,
      fundingLockScript: snapshot.fundingLockScript,
      l1BalanceCkb: snapshot.l1BalanceCkb,
      l1BalanceShannons: snapshot.l1BalanceShannons,
      l1BalanceSource: snapshot.l1BalanceSource,
    });
    const l1Ckb = Number(snapshot.l1BalanceCkb ?? 0);
    const funded = Number.isFinite(l1Ckb) && l1Ckb > 0;
    setStatus(
      root,
      snapshot.source === "simulated"
        ? "Demo mode — this address can't receive real test coins yet"
        : funded
          ? `L1 funding lock shows ${snapshot.l1BalanceCkb} CKB — ready to open channels`
          : "Address ready — claim free coins (then Refresh). Dashboard channel balance stays 0 until a Fiber channel is open.",
      "ok",
    );
  } catch (error) {
    log.error("get_fnn_address failed", error);
    setStatus(root, safeUserMessage(error, "Could not load receive address"), "err");
  }
}

/**
 * @param {HTMLElement} root
 */
async function copyAddress(root) {
  if (!cachedAddress) {
    setStatus(root, "No address loaded yet", "err");
    return;
  }
  try {
    await navigator.clipboard.writeText(cachedAddress);
    setStatus(root, "Address copied to clipboard", "ok");
  } catch (error) {
    log.warn("clipboard write failed", error);
    setStatus(root, safeUserMessage(error, "Clipboard copy failed"), "err");
  }
}

/**
 * @param {HTMLElement} root
 */
async function openFaucet(root) {
  try {
    await openExternalUrl(FAUCET_URL);
    setStatus(
      root,
      "Opened the faucet — paste the address above to claim free coins",
      "ok",
    );
  } catch (error) {
    log.error("open faucet failed", error);
    setStatus(root, safeUserMessage(error, "Could not open the faucet"), "err");
  }
}

/**
 * Open JoyID portal in the system browser when in-app popups are blocked (common in WebViews).
 * @param {HTMLElement} root
 */
async function openJoyIdPortal(root) {
  try {
    await openExternalUrl(JOYID_TESTNET_URL);
    setStatus(
      root,
      "Opened JoyID in your browser — sign in, then return here to send",
      "info",
    );
  } catch (error) {
    log.error("open JoyID portal failed", error);
    setStatus(root, safeUserMessage(error, "Could not open JoyID"), "err");
  }
}

/**
 * @param {any} ccc
 * @param {any} CkbSigner
 * @returns {Promise<any>}
 */
async function connectJoyIdSigner(ccc, CkbSigner) {
  const client = new ccc.ClientPublicTestnet();
  if (activeJoySigner && (await activeJoySigner.isConnected?.())) {
    return activeJoySigner;
  }
  if (!CkbSigner) {
    throw new Error("JoyID SDK failed to load — check network / esm.sh");
  }
  // JoyID opens about:blank then navigates to testnet.joyid.dev via window.open.
  // In Tauri that requires on_new_window Allow (wired in Rust). Still surface a tip.
  if (hasTauri()) {
    log.info("JoyID connect: approve the Request Pop-up, then complete passkey in the JoyID window");
  }
  const signer = new CkbSigner(client, APP_NAME, APP_ICON);
  if (!(await signer.isConnected())) {
    await signer.connect();
  }
  activeJoySigner = signer;
  try {
    const addr = await signer.getInternalAddress?.();
    connectedWalletLabel = addr ? `JoyID · ${String(addr).slice(0, 10)}…${String(addr).slice(-6)}` : "JoyID connected";
  } catch {
    connectedWalletLabel = "JoyID connected";
  }
  return signer;
}

/**
 * Mount CCC wallet picker on document.body (never inside a transformed card).
 * @returns {Promise<any>}
 */
async function openCccWalletPicker() {
  await import("@ckb-ccc/connector");
  return new Promise((resolve, reject) => {
    const existing = document.querySelector("ccc-connector[data-funding-portal]");
    existing?.remove();

    const el = document.createElement("ccc-connector");
    el.setAttribute("data-funding-portal", "1");
    el.setAttribute("name", APP_NAME);
    el.setAttribute("icon", APP_ICON);

    const cleanup = () => {
      el.removeEventListener("close", onClose);
      el.remove();
    };

    const onClose = () => {
      const signer = /** @type {any} */ (el).signer?.signer;
      cleanup();
      if (signer) {
        activeJoySigner = signer;
        connectedWalletLabel = /** @type {any} */ (el).wallet?.name
          ? `${/** @type {any} */ (el).wallet.name} connected`
          : "Wallet connected";
        resolve(signer);
        return;
      }
      reject(new Error("Wallet connection cancelled"));
    };

    el.addEventListener("close", onClose);
    document.body.appendChild(el);
  });
}

/**
 * @param {HTMLElement} root
 * @param {any} signer
 * @param {any} ccc
 * @param {bigint} ckbAmount
 */
async function sendCapacity(root, signer, ccc, ckbAmount) {
  if (!cachedAddress) {
    throw new Error("Load your receive address first");
  }
  const shannons = ckbAmount * SHANNONS_PER_CKB;
  const ckbDisplay = ckbAmount.toString();
  const client = signer.client ?? new ccc.ClientPublicTestnet();
  const toAddr = await ccc.Address.fromString(cachedAddress, client);
  const capacity = ccc.fixedPointFrom ? ccc.fixedPointFrom(ckbDisplay) : shannons;
  let tx = ccc.Transaction.from({
    outputs: [
      {
        lock: toAddr.script,
        capacity,
      },
    ],
  });

  if (typeof signer.prepareTransaction === "function") {
    tx = await signer.prepareTransaction(tx);
  }
  if (typeof tx.completeInputsByCapacity === "function") {
    await tx.completeInputsByCapacity(signer);
  } else if (typeof signer.completeInputs === "function") {
    await signer.completeInputs(tx);
  }
  if (typeof tx.completeFeeBy === "function") {
    await tx.completeFeeBy(signer);
  } else if (typeof tx.completeFee === "function") {
    await tx.completeFee(signer);
  }

  setStatus(root, "Approve on your device…", "info");
  const txHash =
    typeof signer.sendTransaction === "function"
      ? await signer.sendTransaction(tx)
      : await ccc.sendTransaction(signer, tx);

  log.info("JoyID funding submitted", { txHash });
  setStatus(
    root,
    `Sent ${ckbDisplay} CKB — payment submitted`,
    "ok",
  );
}

/**
 * One-tap JoyID: connect (if needed) then transfer.
 * @param {HTMLElement} root
 * @param {{ ccc: any, CkbSigner: any }} api
 */
async function fundFromJoyId(root, api) {
  /** @type {bigint} */
  let ckbAmount;
  try {
    ckbAmount = readAmountCkb(root);
  } catch (error) {
    setStatus(root, safeUserMessage(error, "Enter a whole positive CKB amount"), "err");
    return;
  }
  if (ckbAmount <= 0n) {
    setStatus(root, "Enter a positive CKB amount", "err");
    return;
  }
  if (!cachedAddress) {
    setStatus(root, "Load your receive address first", "err");
    return;
  }

  setStatus(root, "Opening JoyID…", "info");
  try {
    const signer = await connectJoyIdSigner(api.ccc, api.CkbSigner);
    paintWalletChip(root, { connected: true, label: connectedWalletLabel });
    setStatus(root, "Preparing payment…", "info");
    await sendCapacity(root, signer, api.ccc, ckbAmount);
  } catch (error) {
    log.error("JoyID funding failed", error);
    const msg = safeUserMessage(error, "JoyID funding failed");
    const detail = [
      error instanceof Error ? error.message : String(error ?? ""),
      error?.name,
      error?.cause,
    ]
      .filter(Boolean)
      .join(" ");
    const credentialMissing =
      /WalletNotSupportedError|Credential not found|credential/i.test(detail || msg);
    const popupBlocked =
      /popup|webview|standard browsers|blocked|cancelled|Not connected/i.test(detail || msg);

    if (credentialMissing) {
      setStatus(
        root,
        "No JoyID passkey for this app yet — click “+ Create New” in the JoyID window (installed app uses tauri.localhost; that is a different login than browser/dev). Or use Copy address + faucet.",
        "err",
      );
      return;
    }

    setStatus(root, msg, "err");
    if (popupBlocked) {
      await openJoyIdPortal(root);
    }
  }
}

/**
 * @param {HTMLElement} root
 * @param {{ ccc: any }} api
 */
async function fundFromConnectedWallet(root, api) {
  /** @type {bigint} */
  let ckbAmount;
  try {
    ckbAmount = readAmountCkb(root);
  } catch (error) {
    setStatus(root, safeUserMessage(error, "Enter a whole positive CKB amount"), "err");
    return;
  }
  setStatus(root, "Choose a wallet…", "info");
  try {
    const signer = await openCccWalletPicker();
    paintWalletChip(root, { connected: true, label: connectedWalletLabel });
    setStatus(root, "Preparing payment…", "info");
    await sendCapacity(root, signer, api.ccc, ckbAmount);
  } catch (error) {
    log.error("CCC wallet funding failed", error);
    setStatus(root, safeUserMessage(error, "Wallet funding failed"), "err");
  }
}

/**
 * Mount funding onboarding interactions on a panel root.
 * @param {HTMLElement} root
 */
export async function mountFundingPanel(root) {
  const found = root.querySelector("[data-funding-root]");
  const scope = found instanceof HTMLElement ? found : root;

  scope.querySelector("[data-action='funding-refresh']")?.addEventListener("click", () => {
    void refreshFnnAddress(scope);
  });
  scope.querySelector("[data-action='funding-copy']")?.addEventListener("click", () => {
    void copyAddress(scope);
  });
  scope.querySelector("[data-action='funding-faucet']")?.addEventListener("click", () => {
    void openFaucet(scope);
  });
  scope.querySelector("[data-action='funding-amount-dec']")?.addEventListener("click", () => {
    try {
      writeAmountCkb(scope, readAmountCkb(scope) - 10n);
    } catch {
      writeAmountCkb(scope, 1n);
    }
  });
  scope.querySelector("[data-action='funding-amount-inc']")?.addEventListener("click", () => {
    try {
      writeAmountCkb(scope, readAmountCkb(scope) + 10n);
    } catch {
      writeAmountCkb(scope, BigInt(DEFAULT_FUND_CKB));
    }
  });
  scope.querySelector("[data-action='funding-joyid-portal']")?.addEventListener("click", () => {
    void openJoyIdPortal(scope);
  });

  paintWalletChip(scope, { connected: false });
  void refreshFnnAddress(scope);

  let cccApi = null;
  try {
    cccApi = await loadCcc();
  } catch (error) {
    log.warn("CCC SDK unavailable", error);
    setStatus(
      scope,
      "Wallet tools didn't load — free faucet still works",
      "info",
    );
  }

  scope.querySelector("[data-action='funding-joyid']")?.addEventListener("click", () => {
    if (!cccApi?.ccc) {
      setStatus(scope, "Wallet tools not ready", "err");
      return;
    }
    void fundFromJoyId(scope, cccApi);
  });

  scope.querySelector("[data-action='funding-ccc']")?.addEventListener("click", () => {
    if (!cccApi?.ccc || !cccApi.connectorReady) {
      setStatus(scope, "Wallet tools not ready", "err");
      return;
    }
    void fundFromConnectedWallet(scope, cccApi);
  });
}

export function fundingPanelMarkup() {
  return `
    <div class="funding-onboard funding-superapp" data-funding-root>
      <section class="funding-address-bar" aria-label="Receive address">
        <div class="funding-address-bar-copy">
          <p class="funding-kicker">This agent</p>
          <h2 class="funding-address-title">Add funds</h2>
          <p class="panel-hint">Test coins for payment links</p>
        </div>
        <label class="funding-address-label" for="funding-address-display">Your receive address</label>
        <div class="funding-address-row">
          <code id="funding-address-display" class="funding-address" data-funding-address>Fetching…</code>
          <div class="funding-address-actions">
            <button type="button" class="panel-btn" data-action="funding-copy">Copy</button>
            <button type="button" class="panel-btn" data-action="funding-refresh">Refresh</button>
          </div>
        </div>
        <p class="funding-l1-balance" data-funding-l1-balance>L1 funding lock: …</p>
        <p class="panel-hint" data-funding-meta>test coins</p>
        <p class="funding-status" data-funding-status data-tone="info">Loading…</p>
      </section>

      <section class="funding-joy-sheet" aria-labelledby="funding-joy-title">
        <div class="funding-joy-glow" aria-hidden="true"></div>
        <header class="funding-joy-head">
          <p class="funding-kicker">Option 2 · Stay in-app</p>
          <h3 id="funding-joy-title">JoyID passkey</h3>
          <p class="panel-hint">Confirm once on your device to send coins here.</p>
        </header>

        <div class="funding-wallet-chip" data-funding-wallet data-connected="false">Passkey not connected</div>

        <div class="funding-amount-pad">
          <span class="funding-amount-label">Amount</span>
          <div class="funding-amount-controls">
            <button type="button" class="funding-amount-step" data-action="funding-amount-dec" aria-label="Decrease amount">−</button>
            <div class="funding-amount-field">
              <input id="funding-amount-ckb" data-funding-amount inputmode="numeric" pattern="[0-9]*" autocomplete="off" value="${DEFAULT_FUND_CKB}" aria-label="Amount in CKB">
              <span class="funding-amount-unit">CKB</span>
            </div>
            <button type="button" class="funding-amount-step" data-action="funding-amount-inc" aria-label="Increase amount">+</button>
          </div>
        </div>

        <button type="button" class="funding-joy-cta" data-action="funding-joyid">
          Continue with JoyID
        </button>
        <div class="funding-joy-links">
          <button type="button" class="funding-text-btn" data-action="funding-ccc">Other wallets</button>
          <button type="button" class="funding-text-btn" data-action="funding-joyid-portal">Open JoyID website</button>
        </div>
      </section>

      <section class="funding-faucet-rail" aria-labelledby="funding-faucet-title">
        <div class="funding-faucet-copy">
          <p class="funding-kicker">Option 1 · Browser</p>
          <h3 id="funding-faucet-title">Free test coins</h3>
          <p class="panel-hint">Copy the address above, claim free coins, done.</p>
        </div>
        <button type="button" class="panel-btn panel-btn-primary funding-faucet-btn" data-action="funding-faucet">
          Claim free coins
        </button>
      </section>
    </div>
  `;
}

export { escapeHtml };
