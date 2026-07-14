import { createLogger } from "../dashboard/logger.js";
import { parseAtomicInt } from "../dashboard/money.js";
import { escapeHtml, safeUserMessage } from "./dom-security.js";
import { getFnnAddress, hasTauri, openExternalUrl } from "./sidecar-api.js";

const log = createLogger("funding");

const FAUCET_URL = "https://faucet.nervos.org/";
/** CKB → shannons (1 CKB = 10^8 shannons). */
const SHANNONS_PER_CKB = 100_000_000n;
/** Default JoyID → sidecar funding amount (whole CKB). */
const DEFAULT_FUND_CKB = 100;

/** @type {string} */
let cachedAddress = "";
/** @type {Record<string, unknown> | null} */
let cachedLockScript = null;

/**
 * Lazy-load CCC modules via the HTML import map (esm.sh fallback).
 * @returns {Promise<{ ccc: any, connectorReady: boolean }>}
 */
async function loadCcc() {
  const [{ ccc }, connector] = await Promise.all([
    import("@ckb-ccc/core"),
    import("@ckb-ccc/connector").catch(() => null),
  ]);
  return { ccc, connectorReady: Boolean(connector) };
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
 * @param {{ address: string, pubkey?: string, source?: string, network?: string, fundingLockScript?: Record<string, unknown> }} snapshot
 */
function paintAddress(root, snapshot) {
  cachedAddress = snapshot.address;
  cachedLockScript = snapshot.fundingLockScript ?? null;
  const addrEl = root.querySelector("[data-funding-address]");
  if (addrEl) addrEl.textContent = snapshot.address;
  const metaEl = root.querySelector("[data-funding-meta]");
  if (metaEl) {
    const parts = [
      snapshot.network ?? "testnet",
      snapshot.source ?? "fnn",
      snapshot.pubkey ? `pubkey ${String(snapshot.pubkey).slice(0, 12)}…` : null,
    ].filter(Boolean);
    metaEl.textContent = parts.join(" · ");
  }
}

/**
 * @param {HTMLElement} root
 */
export async function refreshFnnAddress(root) {
  setStatus(root, "Fetching FNN funding address…", "info");
  try {
    if (!hasTauri()) {
      throw new Error("Tauri runtime unavailable — open the desktop sidecar shell");
    }
    const snapshot = await getFnnAddress();
    paintAddress(root, {
      address: snapshot.address,
      pubkey: snapshot.pubkey,
      source: snapshot.source,
      network: snapshot.network,
      fundingLockScript: snapshot.fundingLockScript,
    });
    setStatus(
      root,
      snapshot.source === "simulated"
        ? "Simulate mode — address is display-only until FNN_MODE=testnet"
        : "Address ready — claim faucet CKB or fund via JoyID",
      "ok",
    );
  } catch (error) {
    log.error("get_fnn_address failed", error);
    setStatus(root, safeUserMessage(error, "Could not load FNN address"), "err");
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
      "Opened Nervos faucet in your browser — paste the sidecar address to claim",
      "ok",
    );
  } catch (error) {
    log.error("open faucet failed", error);
    setStatus(root, safeUserMessage(error, "Could not open faucet"), "err");
  }
}

/**
 * @param {HTMLElement} root
 * @param {any} ccc
 */
async function fundFromJoyId(root, ccc) {
  const amountInput = root.querySelector("[data-funding-amount]");
  const rawAmount =
    amountInput instanceof HTMLInputElement ? amountInput.value.trim() : String(DEFAULT_FUND_CKB);
  /** @type {bigint} */
  let ckbAmount;
  try {
    ckbAmount = parseAtomicInt(rawAmount, "CKB");
  } catch (error) {
    setStatus(root, safeUserMessage(error, "Enter a whole positive CKB amount"), "err");
    return;
  }
  if (ckbAmount <= 0n) {
    setStatus(root, "Enter a positive CKB amount", "err");
    return;
  }
  if (!cachedAddress) {
    setStatus(root, "Load the FNN address before funding", "err");
    return;
  }

  setStatus(root, "Connecting JoyID / CCC signer…", "info");
  try {
    const client = new ccc.ClientPublicTestnet();
    /** @type {any} */
    let signer =
      typeof ccc.getSigner === "function" ? await ccc.getSigner(client) : null;

    if (!signer && typeof ccc.connector?.getConnectedSigner === "function") {
      signer = await ccc.connector.getConnectedSigner(client);
    }
    if (!signer) {
      const signerFactory = ccc.Signer?.fromClient ?? ccc.Wallet?.findSigner;
      if (typeof signerFactory === "function") {
        signer = await signerFactory(client);
      }
    }
    if (!signer) {
      throw new Error(
        "No CCC signer connected — use the Connect wallet control, then try again",
      );
    }

    const shannons = ckbAmount * SHANNONS_PER_CKB;
    const ckbDisplay = ckbAmount.toString();
    const toAddr = await ccc.Address.fromString(cachedAddress, client);
    const tx = ccc.Transaction.from({
      outputs: [
        {
          lock: toAddr.script,
          capacity: ccc.fixedPointFrom
            ? ccc.fixedPointFrom(ckbDisplay)
            : shannons,
        },
      ],
    });

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

    setStatus(root, "Signing JoyID transfer…", "info");
    const txHash =
      typeof signer.sendTransaction === "function"
        ? await signer.sendTransaction(tx)
        : await ccc.sendTransaction(signer, tx);

    log.info("JoyID funding submitted", { txHash });
    setStatus(
      root,
      `Funded ${ckbDisplay} CKB → ${cachedAddress.slice(0, 14)}… · tx ${String(txHash).slice(0, 18)}…`,
      "ok",
    );
  } catch (error) {
    log.error("JoyID funding failed", error);
    setStatus(root, safeUserMessage(error, "JoyID funding failed"), "err");
  }
}

/**
 * Mount funding onboarding interactions on a panel root.
 * @param {HTMLElement} root
 */
export async function mountFundingPanel(root) {
  const refreshBtn = root.querySelector("[data-action='funding-refresh']");
  const copyBtn = root.querySelector("[data-action='funding-copy']");
  const faucetBtn = root.querySelector("[data-action='funding-faucet']");
  const joyBtn = root.querySelector("[data-action='funding-joyid']");

  refreshBtn?.addEventListener("click", () => {
    void refreshFnnAddress(root);
  });
  copyBtn?.addEventListener("click", () => {
    void copyAddress(root);
  });
  faucetBtn?.addEventListener("click", () => {
    void openFaucet(root);
  });

  void refreshFnnAddress(root);

  let cccApi = null;
  try {
    cccApi = await loadCcc();
    const host = root.querySelector("[data-ccc-host]");
    if (host && cccApi.connectorReady) {
      // Ensure the web component is present after dynamic import registered it.
      if (!host.querySelector("ccc-connector")) {
        host.innerHTML = `<ccc-connector></ccc-connector>`;
      }
    }
  } catch (error) {
    log.warn("CCC SDK unavailable", error);
    setStatus(
      root,
      "CCC wallet SDK failed to load — faucet route still works",
      "info",
    );
  }

  joyBtn?.addEventListener("click", () => {
    if (!cccApi?.ccc) {
      setStatus(root, "CCC SDK not loaded", "err");
      return;
    }
    void fundFromJoyId(root, cccApi.ccc);
  });
}

export function fundingPanelMarkup() {
  return `
    <div class="funding-onboard" data-funding-root>
      <div class="funding-hero workspace-card">
        <div class="workspace-card-head">
          <h2>Fund your local FNN</h2>
          <p class="panel-hint">Testnet CKB for channel opens — faucet or JoyID passkey</p>
        </div>
        <label class="funding-address-label" for="funding-address-display">Sidecar funding address</label>
        <div class="funding-address-row">
          <code id="funding-address-display" class="funding-address" data-funding-address>Fetching…</code>
          <button type="button" class="panel-btn" data-action="funding-copy">Copy Address</button>
          <button type="button" class="panel-btn" data-action="funding-refresh">Refresh</button>
        </div>
        <p class="panel-hint" data-funding-meta>testnet</p>
        <p class="funding-status" data-funding-status data-tone="info">Loading…</p>
      </div>

      <div class="funding-options">
        <section class="workspace-card funding-option">
          <div class="workspace-card-head">
            <h3>Option 1 · Nervos faucet</h3>
            <p class="panel-hint">Lowest friction — claim free testnet CKB in your browser</p>
          </div>
          <ol class="funding-steps">
            <li>Copy the sidecar <code>ckt1</code> address above</li>
            <li>Open the official Nervos testnet faucet</li>
            <li>Paste the address and claim</li>
          </ol>
          <button type="button" class="panel-btn panel-btn-primary" data-action="funding-faucet">
            Claim Testnet CKB
          </button>
        </section>

        <section class="workspace-card funding-option">
          <div class="workspace-card-head">
            <h3>Option 2 · JoyID passkey</h3>
            <p class="panel-hint">Stay in-app — CCC connector + on-chain transfer to the sidecar</p>
          </div>
          <div class="funding-ccc-host" data-ccc-host>
            <ccc-connector></ccc-connector>
          </div>
          <div class="workspace-field" style="margin-top:0.75rem">
            <label for="funding-amount-ckb">Amount (CKB)</label>
            <input id="funding-amount-ckb" data-funding-amount type="number" min="1" step="1" value="${DEFAULT_FUND_CKB}">
          </div>
          <button type="button" class="panel-btn panel-btn-primary" data-action="funding-joyid" style="margin-top:0.75rem">
            Fund from JoyID
          </button>
        </section>
      </div>
    </div>
  `;
}

export { escapeHtml };
