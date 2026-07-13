import { complianceStreamUrl, requestComplianceTicket } from "../../mfa-api.js";
import { errorMessage } from "../../../../packages/fsp-ui-types/errors.js";

/** @type {EventSource | null} */
let activeSource = null;

export const compliancePanel = {
  id: "mfa-compliance",
  title: "Compliance Surveillance",
  navLabel: "Surveillance",
  navIcon: "compliance",
  badge: "regulatory",
  navDescription: "Authenticated SSE stream for sovereign audit and compliance tickets.",
  render() {
    return `
      <div class="workspace-card">
        <div class="workspace-card-head">
          <h2>Surveillance stream</h2>
          <p class="panel-hint">EphemeralTicketRegistry · single-use ticket · TTL 30s · burn on SSE connect</p>
        </div>
        <div class="mesh-control-row" style="margin-bottom:0.75rem">
          <button type="button" class="panel-btn panel-btn-primary" data-action="start-stream">Start stream</button>
          <button type="button" class="panel-btn" data-action="stop-stream">Stop</button>
        </div>
        <pre class="compliance-stream" data-compliance-output>Awaiting stream…</pre>
      </div>`;
  },
  /**
   * @param {HTMLElement} root
   */
  mount(root) {
    const output = root.querySelector("[data-compliance-output]");

    /**
     * @param {string} line
     */
    const append = (line) => {
      if (!output) return;
      const prefix = output.textContent === "Awaiting stream…" ? "" : `${output.textContent}\n`;
      output.textContent = `${prefix}${line}`.slice(-12000);
    };

    root.querySelector("[data-action='start-stream']")?.addEventListener("click", async () => {
      try {
        if (activeSource) activeSource.close();
        const ticket = await requestComplianceTicket();
        const ticketValue = ticket.ticket;
        if (!ticketValue) throw new Error("No ephemeral ticket in response");
        const url = complianceStreamUrl(ticketValue);
        activeSource = new EventSource(url);
        activeSource.onmessage = (ev) => append(ev.data);
        activeSource.onerror = () => append("[stream error]");
        append(`[connected] ${url.split("?")[0]}`);
      } catch (err) {
        append(`[error] ${errorMessage(err)}`);
      }
    });

    root.querySelector("[data-action='stop-stream']")?.addEventListener("click", () => {
      activeSource?.close();
      activeSource = null;
      if (output) output.textContent = "Stream stopped.";
    });
  },
};
