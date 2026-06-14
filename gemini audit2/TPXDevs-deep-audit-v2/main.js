import init, { FiberAgent } from "../pkg/fiber_agent.js";

const logEl = document.getElementById("log");
const statusEl = document.getElementById("status");
const btnStart = document.getElementById("btn-start");
const btnStop = document.getElementById("btn-stop");
const metricState = document.getElementById("metric-state");
const metricNeighbors = document.getElementById("metric-neighbors");
const metricPeers = document.getElementById("metric-peers");

let agent = null;
let metricsTimer = null;
let running = false;

function appendLine(text, level = "log") {
  const prefix = level === "error" ? "✖ " : level === "warn" ? "⚠ " : "";
  logEl.textContent += `${prefix}${text}\n`;
  logEl.scrollTop = logEl.scrollHeight;
}

function patchConsole() {
  for (const level of ["log", "warn", "error"]) {
    const orig = console[level].bind(console);
    console[level] = (...args) => {
      orig(...args);
      appendLine(
        args.map((a) => (typeof a === "string" ? a : JSON.stringify(a))).join(" "),
        level,
      );
    };
  }
}

function readU16(id) {
  const el = document.getElementById(id);
  const n = Number(el.value);
  if (!Number.isInteger(n) || n < 1 || n > 1024) {
    throw new Error(`Invalid value for ${id}`);
  }
  return n;
}

/** Same lattice mesh peers as MFA CompleteMeshGraph::new */
function meshPeersFor(agentId, totalNodes = 1024) {
  const i = agentId;
  return [
    i === totalNodes ? 1 : i + 1,
    i >= totalNodes - 1 ? 1 : i + 2,
    ((i + totalNodes / 2 - 1) % totalNodes) + 1,
  ].filter((t, idx, arr) => t !== i && arr.indexOf(t) === idx);
}

function updateMetrics() {
  if (!agent) return;
  metricState.textContent = agent.get_state_string();
  metricNeighbors.textContent = String(agent.get_active_neighbor_count());
}

async function startAgent() {
  if (running) return;

  const agentId = readU16("agent-id");
  const maxFailures = readU16("max-failures");
  const tickMs = Number(document.getElementById("tick-ms").value);
  const mfaUrl = document.getElementById("mfa-url").value.trim();
  const mfaWsUrl = document.getElementById("mfa-ws-url").value.trim();

  if (!mfaUrl) throw new Error("MFA telemetry URL is required.");
  if (!mfaWsUrl) throw new Error("MFA WebSocket base URL is required.");
  if (!Number.isFinite(tickMs) || tickMs < 500) {
    throw new Error("Tick rate must be at least 500 ms.");
  }

  const peers = meshPeersFor(agentId);
  metricPeers.textContent = peers.map((p) => `FA-${p}`).join(", ");

  agent = new FiberAgent(agentId, new Uint16Array(peers), maxFailures);

  try {
    agent.initialize_websocket_link(mfaWsUrl);
    appendLine(`WebSocket uplink → ${mfaWsUrl}/ws/${agentId}`);
  } catch (err) {
    appendLine(`WebSocket failed: ${err}`, "warn");
  }

  running = true;
  btnStart.disabled = true;
  btnStop.disabled = false;
  statusEl.textContent = `Running FA-${agentId} with mesh peers [${peers.join(", ")}]`;

  metricsTimer = setInterval(updateMetrics, 500);
  updateMetrics();

  appendLine(
    `Starting mesh heartbeat: tick=${tickMs}ms, telemetry=${mfaUrl}`,
  );

  agent.start_mesh_heartbeat_loop(tickMs, mfaUrl).catch((err) => {
    appendLine(`Loop error: ${err}`, "error");
    stopAgent();
  });
}

function stopAgent() {
  running = false;
  btnStart.disabled = false;
  btnStop.disabled = true;
  statusEl.textContent = "Stopped — reload page to restart the Wasm agent.";
  if (metricsTimer) {
    clearInterval(metricsTimer);
    metricsTimer = null;
  }
}

btnStart.addEventListener("click", () => {
  startAgent().catch((err) => {
    appendLine(String(err), "error");
    statusEl.textContent = `Error: ${err.message}`;
  });
});

btnStop.addEventListener("click", stopAgent);

patchConsole();
statusEl.textContent = "Loading Wasm…";

try {
  await init();
  statusEl.textContent = "Wasm ready — click Start mesh agent.";
  appendLine("fiber_agent Wasm module initialized.");
} catch (err) {
  statusEl.textContent = `Wasm load failed: ${err.message}`;
  appendLine(`Init failed: ${err}`, "error");
  btnStart.disabled = true;
}
