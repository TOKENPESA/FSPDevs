import { MFA_API_BASE_URL, mfaAuthHeaders } from "./config.js";
import { fetchWithTimeout } from "./fetch-timeout.js";
import { createLogger } from "./logger.js";
import { $ } from "./dom.js";

const log = createLogger("regulatory-core");

document.addEventListener("DOMContentLoaded", () => {
  void initializeRegulatoryStreamListener();
});

let computedTaxAccumulator = 14240500;
/** @type {ReturnType<typeof setTimeout> | null} */
let reconnectTimer = null;

/**
 * @param {string} verdict
 * @param {HTMLElement | null} warningBox
 * @returns {HTMLSpanElement}
 */
function createVerdictBadge(verdict, warningBox) {
  const badge = document.createElement("span");
  if (verdict === "ClearedClean") {
    badge.className = "badge CLEARED";
    badge.textContent = "VERIFIED";
  } else if (verdict === "AuditFlagged") {
    badge.className = "badge FLAGGED";
    badge.textContent = "AUDIT WARN";
  } else {
    badge.className = "badge BLOCKED";
    badge.textContent = "INTERCEPTED";
    if (warningBox) {
      warningBox.style.display = "block";
    }
  }
  return badge;
}

/**
 * @param {HTMLElement | null} terminal
 * @param {Record<string, unknown>} auditPayload
 * @param {HTMLElement | null} warningBox
 */
function appendAuditEntry(terminal, auditPayload, warningBox) {
  if (!terminal) return undefined;
  const cb = /** @type {Record<string, unknown>} */ (auditPayload.central_bank_feed);
  const tax = /** @type {Record<string, unknown>} */ (
    auditPayload.revenue_tax_telemetry ?? auditPayload.revenue_authority_feed
  );
  const verdict = auditPayload.final_verdict;
  if (!cb || !tax || typeof verdict !== "string") return undefined;

  const timestampString = new Date(
    Number(auditPayload.transaction_timestamp) * 1000,
  ).toLocaleTimeString();

  const auditId =
    typeof auditPayload.audit_id === "string"
      ? auditPayload.audit_id.slice(0, 8)
      : String(auditPayload.audit_id ?? "—").slice(0, 8);

  const entryRow = document.createElement("div");
  entryRow.style.marginBottom = "0.8rem";
  entryRow.style.borderBottom = "1px solid #111a30";
  entryRow.style.paddingBottom = "0.5rem";

  const headerLine = document.createElement("div");
  headerLine.append(document.createTextNode(`[${timestampString}] `));
  headerLine.append(createVerdictBadge(verdict, warningBox));
  headerLine.append(document.createTextNode(" "));
  const idSpan = document.createElement("span");
  idSpan.style.color = "#60a5fa";
  idSpan.textContent = `ID: ${auditId}`;
  headerLine.append(idSpan);

  const detailLine = document.createElement("div");
  detailLine.style.paddingLeft = "1rem";
  detailLine.style.color = "#94a3b8";
  detailLine.style.fontSize = "0.8rem";
  detailLine.textContent =
    `🏛️ CB: Corridor ${cb.source_corridor_iso} ──► ${cb.destination_corridor_iso} | Vol: ${Number(cb.volume_fiat_value).toLocaleString()} | MaskedToken: ${cb.masked_kyc_token}\n` +
    `💸 TRA: Mode: ${tax.transaction_type} | Base Tax Collected: +${tax.calculated_sovereign_tax_levy} TZS`;

  entryRow.append(headerLine, detailLine);
  terminal.appendChild(entryRow);
  terminal.scrollTop = terminal.scrollHeight;

  return { cb, tax, verdict };
}

/**
 * @param {HTMLElement | null} corridorVelocity
 * @param {Record<string, unknown>} cb
 */
function updateCorridorVelocity(corridorVelocity, cb) {
  if (!corridorVelocity || cb.macro_velocity_percent == null) return;
  const baseCap = 100_000_000;
  const shannonsPerSec = (Number(cb.macro_velocity_percent) / 100) * baseCap;
  const label =
    shannonsPerSec >= 1_000_000
      ? `${(shannonsPerSec / 1_000_000).toFixed(2)}M`
      : Math.round(shannonsPerSec).toLocaleString();
  corridorVelocity.replaceChildren(
    document.createTextNode(`${label} `),
    Object.assign(document.createElement("span"), {
      style: { fontSize: "1rem" },
      textContent: "Shannons/s",
    }),
  );
}

async function initializeRegulatoryStreamListener() {
  const terminal = $("audit-terminal");
  const taxDisplay = $("tax-total");
  const warningBox = $("blockade-alert");
  const pipelineStatus = $("pipeline-status");
  const corridorVelocity = $("corridor-velocity");
  const mfaApiUrl = MFA_API_BASE_URL;

  if (taxDisplay) {
    taxDisplay.textContent = `TZS ${Math.floor(computedTaxAccumulator).toLocaleString()}`;
  }

  try {
    const ticketResponse = await fetchWithTimeout(`${mfaApiUrl}/compliance/ticket`, {
      method: "POST",
      headers: mfaAuthHeaders({ "Content-Type": "application/json" }),
    });

    if (!ticketResponse.ok) {
      throw new Error(
        `Ticket acquisition rejected: Status ${ticketResponse.status}`,
      );
    }

    const connectionData = await ticketResponse.json();

    const eventSourceRoute = new EventSource(
      `${mfaApiUrl}/api/v1/compliance/stream?ticket=${encodeURIComponent(connectionData.ticket)}`,
    );

    eventSourceRoute.onopen = () => {
      if (pipelineStatus) {
        pipelineStatus.className = "badge CLEARED";
        pipelineStatus.textContent = "🔒 ACTIVE REAL-TIME FEED";
      }
    };

    eventSourceRoute.onmessage = (event) => {
      if (event.data === "fsp_pipeline_heartbeat_pulse") return;

      try {
        const auditPayload = JSON.parse(event.data);
        if (auditPayload.error || auditPayload.warning) return;

        const result = appendAuditEntry(terminal, auditPayload, warningBox);
        if (!result) return;

        updateCorridorVelocity(corridorVelocity, result.cb);

        if (result.verdict !== "SovereignBlocked") {
          computedTaxAccumulator += Number(result.tax.calculated_sovereign_tax_levy);
          if (taxDisplay) {
            taxDisplay.textContent = `TZS ${Math.floor(computedTaxAccumulator).toLocaleString()}`;
          }
        }
      } catch {
        // Gracefully bypass non-json streams like heartbeat pulses
      }
    };

    eventSourceRoute.onerror = (err) => {
      if (pipelineStatus) {
        pipelineStatus.className = "badge BLOCKED";
        pipelineStatus.textContent = "✖ FEED OFFLINE";
      }
      log.error(
        "Stream terminated or ticket burned. Initiating systematic cycle re-entry...",
        err,
      );
      eventSourceRoute.close();
      if (reconnectTimer) return;
      reconnectTimer = setTimeout(() => {
        reconnectTimer = null;
        void initializeRegulatoryStreamListener();
      }, 5000);
    };
  } catch (error) {
    if (pipelineStatus) {
      pipelineStatus.className = "badge BLOCKED";
      pipelineStatus.textContent = "✖ FEED OFFLINE";
    }
    log.error(
      "Failed to authenticate or establish compliance infrastructure streams:",
      error,
    );
    if (reconnectTimer) return;
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      void initializeRegulatoryStreamListener();
    }, 5000);
  }
}
