import { escapeHtml } from "./dom-security.js";

/**
 * Full-screen, un-dismissible fatal gate when live/testnet FNN failed to boot.
 * @param {{ mode?: string, rpcUrl?: string, error?: string | null }} status
 */
export function mountFatalFnnModal(status) {
  const existing = document.getElementById("fnn-fatal-modal");
  if (existing) {
    existing.remove();
  }

  const mode = status?.mode || "testnet";
  const rpc = status?.rpcUrl || "http://127.0.0.1:8227";
  const detail = status?.error || "FATAL: Live Testnet Node Failed to Boot. Please check port 8227.";

  const root = document.createElement("div");
  root.id = "fnn-fatal-modal";
  root.className = "fnn-fatal-modal";
  root.setAttribute("role", "alertdialog");
  root.setAttribute("aria-modal", "true");
  root.setAttribute("aria-labelledby", "fnn-fatal-title");
  root.innerHTML = `
    <div class="fnn-fatal-card">
      <p class="fnn-fatal-kicker">Live testnet required</p>
      <h1 id="fnn-fatal-title">FATAL: Live Testnet Node Failed to Boot. Please check port 8227.</h1>
      <p class="fnn-fatal-lead">
        This Sidecar will not run in demo mode while <code>FNN_MODE=${escapeHtml(mode)}</code>.
        Contributions and payments stay blocked until Fiber Network Node is reachable.
      </p>
      <dl class="fnn-fatal-meta">
        <div><dt>RPC</dt><dd><code>${escapeHtml(rpc)}</code></dd></div>
        <div><dt>Detail</dt><dd>${escapeHtml(detail)}</dd></div>
      </dl>
      <ol class="fnn-fatal-steps">
        <li>In a terminal: <code>cd fnn-testnet</code> then <code>.\\start-testnet.ps1</code></li>
        <li>Or ensure the bundled FNN sidecar is prepared: <code>npm run prepare:fnn-sidecar</code></li>
        <li>Restart Fiber Agent</li>
      </ol>
      <p class="fnn-fatal-foot">Explicit demo only: set <code>FNN_MODE=simulate</code> (not for production).</p>
    </div>
  `;

  document.body.appendChild(root);
  document.body.classList.add("fnn-fatal-locked");

  // Trap focus / block Esc — un-dismissible by design.
  root.addEventListener(
    "keydown",
    (event) => {
      if (event.key === "Escape" || event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        event.stopPropagation();
      }
    },
    true,
  );
}
