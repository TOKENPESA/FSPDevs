import {
  getMfaRuntime,
  loadMfaRuntime,
  MFA_RUNTIME_EVENT,
} from "./mfa-runtime.js";
import { formatLastSync, formatLiveClock } from "./stats-ui.js";
import { MFA_MONITOR_EVENT } from "./monitor-bridge.js";
import { state } from "../../dashboard/state.js";
import { $input } from "../../packages/fsp-ui-types/dom.js";

/** @typedef {import('./types.js').MfaRuntimeDetail} MfaRuntimeDetail */

const MOBILE_MQ = window.matchMedia("(max-width: 960px)");

function syncSidebarBackdrop() {
  const backdrop = document.getElementById("sidebar-backdrop");
  if (!backdrop) return;
  const collapsed = document.body.classList.contains("sidebar-collapsed");
  backdrop.hidden = !(MOBILE_MQ.matches && !collapsed);
}

export function initShell() {
  const toggle = document.getElementById("sidebar-toggle");
  const backdrop = document.getElementById("sidebar-backdrop");

  /** @param {boolean} collapsed */
  const setSidebarCollapsed = (collapsed) => {
    document.body.classList.toggle("sidebar-collapsed", collapsed);
    syncSidebarBackdrop();
  };

  if (MOBILE_MQ.matches) setSidebarCollapsed(true);

  toggle?.addEventListener("click", () => {
    setSidebarCollapsed(!document.body.classList.contains("sidebar-collapsed"));
  });
  backdrop?.addEventListener("click", () => setSidebarCollapsed(true));
  MOBILE_MQ.addEventListener("change", (event) => {
    if (event.matches) setSidebarCollapsed(true);
    else syncSidebarBackdrop();
  });

  initClock();
  initLastSyncBadge();
  initConnPill();
}

export function closeMobileSidebar() {
  if (!MOBILE_MQ.matches) return;
  document.body.classList.add("sidebar-collapsed");
  syncSidebarBackdrop();
}

/**
 * @param {HTMLElement} navEl
 */
export function initNavSearch(navEl) {
  const search = $input("nav-search");
  if (!search || !navEl) return;

  search.addEventListener("input", () => {
    const query = search.value.trim().toLowerCase();
    navEl.querySelectorAll(".nav-group[data-nav-group='supervisor']").forEach((group) => {
      if (!(group instanceof HTMLElement)) return;
      const groupText = group.textContent?.toLowerCase() ?? "";
      let visibleChildren = 0;
      group.querySelectorAll(".nav-tree-item").forEach((item) => {
        if (!(item instanceof HTMLElement)) return;
        const text = item.textContent?.toLowerCase() ?? "";
        const match = !query || text.includes(query) || groupText.includes(query);
        item.style.display = match ? "" : "none";
        if (match) visibleChildren += 1;
      });
      group.querySelectorAll(".nav-subgroup").forEach((subgroup) => {
        if (!(subgroup instanceof HTMLElement)) return;
        const subgroupText = subgroup.textContent?.toLowerCase() ?? "";
        let visibleLeaves = 0;
        subgroup.querySelectorAll(".nav-tree-item").forEach((item) => {
          if (!(item instanceof HTMLElement)) return;
          const text = item.textContent?.toLowerCase() ?? "";
          const match =
            !query || text.includes(query) || subgroupText.includes(query) || groupText.includes(query);
          item.style.display = match ? "" : "none";
          if (match) visibleLeaves += 1;
        });
        subgroup.style.display =
          !query || subgroupText.includes(query) || groupText.includes(query) || visibleLeaves > 0
            ? ""
            : "none";
        if (visibleLeaves > 0) visibleChildren += visibleLeaves;
      });
      group.style.display =
        !query || groupText.includes(query) || visibleChildren > 0 ? "" : "none";
    });
    navEl.querySelectorAll(":scope > .nav-item:not(.nav-group-trigger)").forEach((item) => {
      if (!(item instanceof HTMLElement)) return;
      const text = item.textContent?.toLowerCase() ?? "";
      item.style.display = !query || text.includes(query) ? "" : "none";
    });
  });
}

function initClock() {
  const badge = document.getElementById("clock-badge");
  if (!badge) return;
  const tick = () => {
    const now = new Date();
    badge.textContent = formatLiveClock(now);
    badge.setAttribute("datetime", now.toISOString());
  };
  tick();
  window.setInterval(tick, 1000);
}

/**
 * @param {HTMLElement | null} badge
 * @param {MfaRuntimeDetail | null | undefined} runtime
 */
function paintLastSyncBadge(badge, runtime) {
  if (!badge) return;
  const unix = runtime?.collectedAtUnix;
  if (unix == null || Number(unix) <= 0) {
    badge.textContent = "Last sync · pending";
    badge.removeAttribute("datetime");
    return;
  }
  badge.textContent = `Last sync · ${formatLastSync(unix)}`;
  badge.setAttribute("datetime", new Date(Number(unix) * 1000).toISOString());
}

/** @param {MfaRuntimeDetail | null | undefined} [runtime] */
export function refreshLastSyncBadge(runtime = getMfaRuntime()) {
  paintLastSyncBadge(document.getElementById("last-sync-badge"), runtime);
}

function initLastSyncBadge() {
  const badge = document.getElementById("last-sync-badge");
  if (!badge) return;
  refreshLastSyncBadge();
  window.addEventListener(MFA_RUNTIME_EVENT, (event) => {
    const detail = /** @type {CustomEvent<MfaRuntimeDetail | null>} */ (event).detail;
    paintLastSyncBadge(badge, detail);
  });
  void loadMfaRuntime().then((runtime) => paintLastSyncBadge(badge, runtime));
}

function initConnPill() {
  const status = document.getElementById("conn-status");
  const dot = document.getElementById("conn-dot");
  /** @param {string} [label] */
  const paint = (label) => {
    const connected = label
      ? label === "connected"
      : state.ws?.readyState === WebSocket.OPEN;
    const connecting = label === "connecting" || state.ws?.readyState === WebSocket.CONNECTING;
    if (status) {
      status.textContent = connected
        ? "Monitor live"
        : connecting
          ? "Connecting…"
          : "Monitor offline";
    }
    dot?.classList.toggle("connected", connected);
  };
  paint();
  window.setInterval(() => paint(), 1500);
  window.addEventListener(MFA_MONITOR_EVENT, (event) => {
    const detail = /** @type {CustomEvent<string>} */ (event).detail;
    paint(detail);
  });
}
