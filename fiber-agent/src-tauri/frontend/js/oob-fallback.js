import { createLogger } from "../dashboard/logger.js";
import { escapeHtml } from "./dom-security.js";
import { icon } from "./icons.js";
import { errorMessage } from "../packages/fsp-ui-types/errors.js";
import {
  generateOobFallbackUri,
  getSidecarStats,
  processOobFallback,
  resolveDicobaMemberId,
} from "./sidecar-api.js";
import { dicobaMemberIdForAgent } from "./dicoba-member-id.js";

/** @typedef {import("../../../../dashboard/types.js").SidecarRuntimeStats} SidecarRuntimeStats */

const log = createLogger("oob-fallback");

const memberIdCache = new Map();
let payloadSyncGeneration = 0;

/**
 * @template {(...args: never[]) => void} T
 * @param {T} fn
 * @param {number} [ms]
 * @returns {(...args: Parameters<T>) => void}
 */
function debounce(fn, ms = 120) {
  /** @type {ReturnType<typeof setTimeout> | null} */
  let timer = null;
  return (...args) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
}

/** @param {number | string} agentId @returns {Promise<string>} */
async function cachedDicobaMemberId(agentId) {
  const id = Number(agentId);
  if (!Number.isInteger(id) || id < 1 || id > 1024) {
    return "";
  }
  if (!memberIdCache.has(id)) {
    memberIdCache.set(id, dicobaMemberIdForAgent(id));
  }
  return memberIdCache.get(id);
}

/**
 * @param {number | string} targetAgentId
 * @param {SidecarRuntimeStats | null} [stats]
 * @returns {Promise<string>}
 */
async function resolveGuarantorMemberId(targetAgentId, stats = null) {
  const agentId = Number(targetAgentId);
  if (!agentId) return "";

  if (stats?.meshPeerAgentId === agentId && stats?.meshPeerDicobaMemberId) {
    return stats.meshPeerDicobaMemberId;
  }

  try {
    return await resolveDicobaMemberId(agentId);
  } catch {
    return cachedDicobaMemberId(agentId);
  }
}

const MODULE_LABELS = {
  dicoba: "DiCoBa loans",
  fiat_bridge: "Fiat bridge",
};

const METHOD_LABELS = {
  request_guarantor_signature: "Request guarantor signature",
  dispatch_float_crisis_clearing: "Dispatch float crisis clearing",
  process_cash_in: "Process cash-in",
};

/** True when backend returned SVG QR markup (may include an XML declaration). */
/** @param {unknown} markup @returns {boolean} */
function isQrSvgMarkup(markup) {
  const raw = String(markup ?? "").trim();
  return raw.includes("<svg") && raw.includes("</svg>");
}

/** Paint backend SVG markup; rejects raw color-token grids from legacy renderers. */
/** @param {HTMLElement | null} container @param {unknown} svgMarkup @returns {boolean} */
function paintQrSvg(container, svgMarkup) {
  if (!container) return false;
  const raw = String(svgMarkup ?? "").trim();
  if (!isQrSvgMarkup(raw)) {
    container.innerHTML = "";
    container.hidden = true;
    if (raw) {
      log.warn("QR markup was not SVG", raw.slice(0, 80));
    }
    return false;
  }
  container.hidden = false;
  container.innerHTML = raw;
  return true;
}

/** @param {string} guarantorMemberId @param {string} [loanId] @returns {Record<string, unknown>} */
function defaultDicobaOobPayload(guarantorMemberId, loanId = "loan-local") {
  return {
    loan_id: loanId,
    guarantor_member_id: guarantorMemberId,
    principal_shannons: 1_900_000,
  };
}

/**
 * @param {string} existingJson
 * @param {string} guarantorMemberId
 * @returns {Record<string, unknown>}
 */
function mergeGuarantorIntoPayload(existingJson, guarantorMemberId) {
  let payload = defaultDicobaOobPayload(guarantorMemberId);
  if (existingJson?.trim()) {
    try {
      const parsed = JSON.parse(existingJson);
      if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
        payload = {
          ...payload,
          ...parsed,
          guarantor_member_id: guarantorMemberId,
        };
      }
    } catch {
      /* keep defaults */
    }
  }
  return payload;
}

/**
 * @param {HTMLElement} card
 * @param {number | string} targetAgentId
 * @param {SidecarRuntimeStats | null} [stats]
 */
async function syncDicobaOobPayload(card, targetAgentId, stats = null) {
  const payloadField = /** @type {HTMLTextAreaElement | null} */ (
    card.querySelector("[data-oob-payload]")
  );
  const hintEl = card.querySelector("[data-oob-guarantor-hint]");
  if (!payloadField) return;

  const agentId = Number(targetAgentId);
  if (!Number.isInteger(agentId) || agentId < 1 || agentId > 1024) {
    if (hintEl) {
      hintEl.textContent = "Enter a valid target agent (FA-1 … FA-1024).";
    }
    return;
  }

  const generation = ++payloadSyncGeneration;
  if (hintEl) {
    hintEl.textContent = `Resolving guarantor_member_id for FA-${agentId}…`;
  }

  try {
    const guarantorMemberId = await resolveGuarantorMemberId(agentId, stats);
    if (generation !== payloadSyncGeneration) return;

    const payload = mergeGuarantorIntoPayload(payloadField.value, guarantorMemberId);
    payloadField.value = JSON.stringify(payload, null, 2);
    if (hintEl) {
      hintEl.innerHTML = `Auto-linked to <strong>FA-${agentId}</strong>: <code>${escapeHtml(guarantorMemberId)}</code>`;
    }
  } catch (error) {
    if (generation !== payloadSyncGeneration) return;
    log.warn("could not resolve guarantor member id", error);
    if (hintEl) {
      hintEl.textContent = "Could not resolve guarantor member id for target agent.";
    }
  }
}

/** @param {{ qrSvg?: string, qr_svg?: string } | null | undefined} result @returns {string} */
function readQrSvgFromResult(result) {
  return result?.qrSvg ?? result?.qr_svg ?? "";
}

/** @param {HTMLElement | null} el @param {string} message @param {"neutral" | "success" | "error" | "warn"} [tone] */
function setOobStatus(el, message, tone = "neutral") {
  if (!el) return;
  el.textContent = message;
  el.dataset.tone = tone;
  el.hidden = !message;
}

export function renderOobFallbackCard() {
  return `
    <section class="workspace-card oob-fallback-card" data-oob-fallback-card>
      <div class="oob-hero">
        <div class="oob-hero-icon" aria-hidden="true">${icon("float", 24)}</div>
        <div class="oob-hero-copy">
          <span class="oob-hero-tag">Works without MFA</span>
          <h2>Send offline to another agent</h2>
          <p>When the network is down, create a QR or link and share it by phone, SMS, or chat. The other agent pastes it here to run the action.</p>
        </div>
      </div>

      <nav class="oob-step-rail" aria-label="Transfer steps">
        <div class="oob-step-rail-item">
          <span class="oob-step-dot">1</span>
          <span class="oob-step-label">Create</span>
        </div>
        <span class="oob-step-line" aria-hidden="true"></span>
        <div class="oob-step-rail-item">
          <span class="oob-step-dot">2</span>
          <span class="oob-step-label">Share</span>
        </div>
        <span class="oob-step-line" aria-hidden="true"></span>
        <div class="oob-step-rail-item">
          <span class="oob-step-dot">3</span>
          <span class="oob-step-label">Receive</span>
        </div>
      </nav>

      <div class="oob-grid">
        <div class="oob-panel" data-step="1">
          <h3 class="oob-panel-title">What do you want to send?</h3>
          <p class="oob-panel-lead">Pick the service, recipient agent, and action details.</p>

          <div class="oob-field">
            <label for="oob-target-module">Service</label>
            <select id="oob-target-module" data-oob-target-module class="oob-control">
              <option value="dicoba">${MODULE_LABELS.dicoba}</option>
              <option value="fiat_bridge">${MODULE_LABELS.fiat_bridge}</option>
            </select>
          </div>

          <div class="oob-field-row">
            <div class="oob-field oob-field--compact">
              <label for="oob-target-agent">Send to agent</label>
              <div class="oob-input-prefix">
                <span>FA-</span>
                <input id="oob-target-agent" type="number" data-oob-target-agent min="1" max="1024" value="12" class="oob-control" aria-describedby="oob-guarantor-hint">
              </div>
            </div>
            <div class="oob-field">
              <label for="oob-method">Action</label>
              <select id="oob-method" data-oob-method class="oob-control">
                <option value="request_guarantor_signature">${METHOD_LABELS.request_guarantor_signature}</option>
                <option value="dispatch_float_crisis_clearing">${METHOD_LABELS.dispatch_float_crisis_clearing}</option>
                <option value="process_cash_in">${METHOD_LABELS.process_cash_in}</option>
              </select>
            </div>
          </div>

          <div class="oob-field">
            <label for="oob-payload">Message data (JSON)</label>
            <textarea id="oob-payload" data-oob-payload rows="4" class="oob-control oob-control--code" placeholder="Resolving guarantor member id…"></textarea>
            <span class="oob-field-hint" id="oob-guarantor-hint" data-oob-guarantor-hint>Changing <strong>Send to agent</strong> updates <code>guarantor_member_id</code> in the JSON below.</span>
          </div>

          <button type="button" class="oob-action oob-action--primary" data-action="oob-generate">
            ${icon("modules", 18)}
            <span>Create QR code</span>
          </button>
        </div>

        <div class="oob-panel oob-panel--share" data-step="2">
          <h3 class="oob-panel-title">Share with the other agent</h3>
          <p class="oob-panel-lead">They scan the QR with a phone, or you copy the link below.</p>

          <div class="oob-qr-host" data-oob-qr-host hidden>
            <div class="oob-qr-frame">
              <div class="oob-qr-svg" data-oob-qr-svg></div>
            </div>
          </div>

          <div class="oob-qr-placeholder" data-oob-qr-placeholder>
            <div class="oob-qr-placeholder-art" aria-hidden="true">▦</div>
            <p class="oob-qr-placeholder-title">No QR yet</p>
            <p class="oob-qr-placeholder-copy">Complete step 1 and tap <strong>Create QR code</strong>.</p>
          </div>

          <div class="oob-share-status" data-oob-share-status hidden></div>

          <div class="oob-field">
            <label for="oob-uri-output">Share link</label>
            <textarea id="oob-uri-output" class="oob-control oob-control--code" data-oob-uri-output readonly rows="3" placeholder="Your link will appear here…"></textarea>
          </div>

          <button type="button" class="oob-action oob-action--secondary" data-action="oob-copy" disabled>
            ${icon("chat", 18)}
            <span>Copy link</span>
          </button>
        </div>

        <div class="oob-panel oob-panel--receive" data-step="3">
          <h3 class="oob-panel-title">Receive on this device</h3>
          <p class="oob-panel-lead">Paste a link someone sent you, then run it on this sidecar.</p>

          <div class="oob-field">
            <label for="oob-import">Paste link here</label>
            <textarea id="oob-import" data-oob-import rows="4" class="oob-control oob-control--code" placeholder="fsp://oob?data=…"></textarea>
          </div>

          <button type="button" class="oob-action oob-action--primary" data-action="oob-import">
            ${icon("mobile", 18)}
            <span>Run on this device</span>
          </button>

          <div class="oob-status" data-oob-status data-tone="neutral">
            Ready to receive a link.
          </div>
        </div>
      </div>
    </section>
  `;
}

/**
 * @param {HTMLSelectElement} moduleSelect
 * @param {HTMLSelectElement} methodSelect
 */
function syncMethodOptions(moduleSelect, methodSelect) {
  const moduleId = moduleSelect.value;
  const options =
    moduleId === "fiat_bridge"
      ? [
          ["dispatch_float_crisis_clearing", METHOD_LABELS.dispatch_float_crisis_clearing],
          ["process_cash_in", METHOD_LABELS.process_cash_in],
        ]
      : [["request_guarantor_signature", METHOD_LABELS.request_guarantor_signature]];

  methodSelect.innerHTML = options
    .map(([value, label]) => `<option value="${value}">${label}</option>`)
    .join("");
}

/** @param {HTMLElement} root */
export async function mountOobFallbackCard(root) {
  const card = root.querySelector("[data-oob-fallback-card]");
  if (!(card instanceof HTMLElement)) return;

  const moduleSelect = /** @type {HTMLSelectElement | null} */ (
    card.querySelector("[data-oob-target-module]")
  );
  const methodSelect = /** @type {HTMLSelectElement | null} */ (
    card.querySelector("[data-oob-method]")
  );
  const uriOutput = /** @type {HTMLTextAreaElement | null} */ (
    card.querySelector("[data-oob-uri-output]")
  );
  const qrHost = /** @type {HTMLElement | null} */ (card.querySelector("[data-oob-qr-host]"));
  const qrPlaceholder = /** @type {HTMLElement | null} */ (
    card.querySelector("[data-oob-qr-placeholder]")
  );
  const qrSvg = /** @type {HTMLElement | null} */ (card.querySelector("[data-oob-qr-svg]"));
  const status = /** @type {HTMLElement | null} */ (card.querySelector("[data-oob-status]"));
  const shareStatus = /** @type {HTMLElement | null} */ (
    card.querySelector("[data-oob-share-status]")
  );
  const copyBtn = /** @type {HTMLButtonElement | null} */ (
    card.querySelector("[data-action='oob-copy']")
  );
  const targetAgentInput = /** @type {HTMLInputElement | null} */ (
    card.querySelector("[data-oob-target-agent]")
  );
  if (!moduleSelect || !methodSelect) return;
  /** @type {SidecarRuntimeStats | null} */
  let runtimeStats = null;

  const refreshDicobaPayload = () => {
    if (moduleSelect?.value !== "dicoba") return;
    const targetAgent = Number(targetAgentInput?.value);
    void syncDicobaOobPayload(card, targetAgent, runtimeStats);
  };
  const refreshDicobaPayloadDebounced = debounce(refreshDicobaPayload, 120);

  moduleSelect.addEventListener("change", () => {
    syncMethodOptions(moduleSelect, methodSelect);
    refreshDicobaPayload();
  });

  targetAgentInput?.addEventListener("input", refreshDicobaPayloadDebounced);
  targetAgentInput?.addEventListener("change", refreshDicobaPayload);
  targetAgentInput?.addEventListener("keyup", refreshDicobaPayloadDebounced);

  try {
    runtimeStats = await getSidecarStats();
    const meshPeer = runtimeStats?.meshPeerAgentId;
    if (meshPeer && targetAgentInput) {
      targetAgentInput.value = String(meshPeer);
    }
    const mounted = runtimeStats?.mountedModules ?? [];
    if (moduleSelect && mounted.length > 0) {
      for (const option of moduleSelect.options) {
        option.disabled = !mounted.includes(option.value);
      }
      const firstMounted = mounted.find((/** @type {string} */ id) => id === "dicoba" || id === "fiat_bridge");
      if (firstMounted) {
        moduleSelect.value = firstMounted;
        syncMethodOptions(moduleSelect, methodSelect);
      }
    }
    refreshDicobaPayload();
  } catch (error) {
    log.warn("could not prefill OOB card from stats", error);
    refreshDicobaPayload();
  }

  card.querySelector("[data-action='oob-generate']")?.addEventListener("click", async () => {
    try {
      if (moduleSelect.value === "dicoba") {
        const targetInput = /** @type {HTMLInputElement | null} */ (
          card.querySelector("[data-oob-target-agent]")
        );
        await syncDicobaOobPayload(
          card,
          Number(targetInput?.value ?? 0),
          runtimeStats,
        );
      }
      const payloadField = /** @type {HTMLTextAreaElement | null} */ (
        card.querySelector("[data-oob-payload]")
      );
      const targetInput = /** @type {HTMLInputElement | null} */ (
        card.querySelector("[data-oob-target-agent]")
      );
      if (!payloadField || !targetInput) return;

      const payload = JSON.parse(payloadField.value);
      const result = await generateOobFallbackUri({
        targetModule: moduleSelect.value,
        targetAgent: Number(targetInput.value),
        method: methodSelect.value,
        payload,
      });

      if (uriOutput) uriOutput.value = result.uri;
      if (copyBtn) copyBtn.disabled = false;
      const svgMarkup = readQrSvgFromResult(result);
        if (qrSvg && svgMarkup) {
        const painted = paintQrSvg(qrSvg, svgMarkup);
        if (qrHost) qrHost.hidden = !painted;
        if (qrPlaceholder) qrPlaceholder.hidden = painted;
      }
      setOobStatus(shareStatus, "QR ready — share it with the other agent.", "success");
    } catch (error) {
      setOobStatus(shareStatus, `Could not create QR: ${errorMessage(error)}`, "error");
      log.error("OOB generate failed", error);
    }
  });

  copyBtn?.addEventListener("click", async () => {
    if (!uriOutput?.value) return;
    try {
      await navigator.clipboard.writeText(uriOutput.value);
      setOobStatus(shareStatus, "Link copied to clipboard.", "success");
    } catch {
      uriOutput.select();
      setOobStatus(shareStatus, "Select the link and copy it manually.", "warn");
    }
  });

  card.querySelector("[data-action='oob-import']")?.addEventListener("click", async () => {
    const importField = /** @type {HTMLTextAreaElement | null} */ (
      card.querySelector("[data-oob-import]")
    );
    const uri = importField?.value?.trim();
    if (!uri) {
      setOobStatus(status, "Paste a link first.", "warn");
      return;
    }
    try {
      const message = await processOobFallback(uri);
      setOobStatus(status, message ?? "Action completed on this device.", "success");
    } catch (error) {
      setOobStatus(status, `Could not run: ${errorMessage(error)}`, "error");
      log.error("OOB import failed", error);
    }
  });
}

/**
 * Generate and display an OOB URI inside any panel log/host element.
 * @param {HTMLElement | null} hostEl
 * @param {{ targetModule: string, targetAgent: number, method: string, payload: Record<string, unknown> }} params
 */
export async function showOobFallbackInHost(
  hostEl,
  { targetModule, targetAgent, method, payload },
) {
  if (!hostEl) return;

  hostEl.style.display = "block";
  hostEl.innerHTML = '<p class="oob-inline-loading">Creating offline share link…</p>';

  try {
    const result = await generateOobFallbackUri({
      targetModule,
      targetAgent,
      method,
      payload,
    });

    hostEl.innerHTML = `
      <div class="oob-inline-result">
        <p class="oob-inline-title"><strong>MFA offline</strong> — share this with FA-${targetAgent}</p>
        <div class="oob-qr-svg oob-qr-svg--inline" data-oob-inline-qr></div>
        <textarea class="oob-control oob-control--code" readonly rows="2"></textarea>
        <p class="oob-field-hint">They scan the QR or paste the link in <strong>Receive on this device</strong>.</p>
      </div>
    `;
    paintQrSvg(hostEl.querySelector("[data-oob-inline-qr]"), readQrSvgFromResult(result));
    const field = /** @type {HTMLTextAreaElement | null} */ (hostEl.querySelector(".oob-control"));
    if (field) field.value = result.uri;
  } catch (error) {
    hostEl.innerHTML = `<p class="oob-inline-error">Could not create link: ${escapeHtml(errorMessage(error))}</p>`;
  }
}
