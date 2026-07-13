import { fetchMfaHealth } from "./mfa-api.js";
import { connectMonitor } from "../../dashboard/events/monitor.js";
import { state } from "../../dashboard/state.js";

export const MFA_MONITOR_EVENT = "mfa-monitor-status";

export function monitorStatusLabel() {
  const ws = state.ws;
  if (!ws) return "offline";
  if (ws.readyState === WebSocket.OPEN) return "connected";
  if (ws.readyState === WebSocket.CONNECTING) return "connecting";
  return "disconnected";
}

export function meshCanvasHintText() {
  switch (monitorStatusLabel()) {
    case "connected":
      return `Live heartbeats · ${state.comm.nodes.size} node(s) on mesh`;
    case "connecting":
      return "Connecting monitor WebSocket…";
    default:
      return "Connect monitor for live heartbeats (MFA on 127.0.0.1:1025)";
  }
}

export function paintMeshCanvasHints() {
  document.querySelectorAll("[data-mesh-canvas-hint]").forEach((el) => {
    el.textContent = meshCanvasHintText();
  });
  window.dispatchEvent(new CustomEvent(MFA_MONITOR_EVENT, { detail: monitorStatusLabel() }));
}

export async function tryAutoConnectMonitor() {
  if (state.ws?.readyState === WebSocket.OPEN) {
    paintMeshCanvasHints();
    return true;
  }

  try {
    await fetchMfaHealth(4000);
  } catch {
    paintMeshCanvasHints();
    return false;
  }

  await connectMonitor();
  paintMeshCanvasHints();
  return state.ws?.readyState === WebSocket.OPEN;
}

/** @type {number | null} */
let hintTimer = null;
/** @type {number | null} */
let autoConnectTimer = null;

export function startMeshHintWatcher() {
  if (hintTimer) return;
  paintMeshCanvasHints();
  hintTimer = window.setInterval(paintMeshCanvasHints, 2000);

  if (autoConnectTimer) return;
  autoConnectTimer = window.setInterval(() => {
    const status = monitorStatusLabel();
    if (status === "connected" || status === "connecting") return;
    void tryAutoConnectMonitor();
  }, 5000);
}
