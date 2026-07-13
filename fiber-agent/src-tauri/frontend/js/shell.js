import {
  formatBrandSubtitle,
  formatBrandSubtitleError,
  getSidecarRuntime,
  loadSidecarRuntime,
  SIDECAR_RUNTIME_EVENT,
} from "./sidecar-runtime.js";
import { formatLastSync, formatLiveClock } from "./stats-ui.js";
import { $input } from "../packages/fsp-ui-types/dom.js";

/** @typedef {import("../../../../dashboard/types.js").SidecarRuntimeStats} SidecarRuntimeStats */

const MOBILE_MQ = window.matchMedia("(max-width: 960px)");

function syncSidebarBackdrop() {
  const backdrop = document.getElementById("sidebar-backdrop");
  if (!backdrop) return;

  const collapsed = document.body.classList.contains("sidebar-collapsed");
  const showBackdrop = MOBILE_MQ.matches && !collapsed;
  backdrop.hidden = !showBackdrop;
}

/** @param {{ onSidebarToggle?: (collapsed: boolean) => void }} [options] */
export function initShell({ onSidebarToggle } = {}) {
  const toggle = document.getElementById("sidebar-toggle");
  const backdrop = document.getElementById("sidebar-backdrop");

  /** @param {boolean} collapsed */
  const setSidebarCollapsed = (collapsed) => {
    document.body.classList.toggle("sidebar-collapsed", collapsed);
    syncSidebarBackdrop();
    onSidebarToggle?.(collapsed);
  };

  if (MOBILE_MQ.matches) {
    setSidebarCollapsed(true);
  }

  toggle?.addEventListener("click", () => {
    setSidebarCollapsed(!document.body.classList.contains("sidebar-collapsed"));
  });

  backdrop?.addEventListener("click", () => {
    setSidebarCollapsed(true);
  });

  MOBILE_MQ.addEventListener("change", (event) => {
    if (event.matches) {
      setSidebarCollapsed(true);
    } else {
      syncSidebarBackdrop();
    }
  });

  initClock();
  initLastSyncBadge();
  initBrandSubtitle();
}

export function closeMobileSidebar() {
  if (!MOBILE_MQ.matches) return;
  document.body.classList.add("sidebar-collapsed");
  syncSidebarBackdrop();
}

/** @param {HTMLElement | null} navEl */
export function initNavSearch(navEl) {
  const search = $input("nav-search");
  if (!search || !navEl) return;

  search.addEventListener("input", () => {
    const query = search.value.trim().toLowerCase();

    navEl.querySelectorAll(".nav-group[data-nav-group='modules']").forEach((group) => {
      const groupText = group.textContent?.toLowerCase() ?? "";
      const childItems = group.querySelectorAll(".nav-tree-item");
      const subgroups = group.querySelectorAll(".nav-subgroup");
      let visibleChildren = 0;

      childItems.forEach((item) => {
        const el = /** @type {HTMLElement} */ (item);
        const text = el.textContent?.toLowerCase() ?? "";
        const match = !query || text.includes(query) || groupText.includes(query);
        el.style.display = match ? "" : "none";
        if (match) visibleChildren += 1;
      });

      subgroups.forEach((subgroup) => {
        const subgroupText = subgroup.textContent?.toLowerCase() ?? "";
        const leaves = subgroup.querySelectorAll(".nav-tree-item");
        let visibleLeaves = 0;

        leaves.forEach((item) => {
          const el = /** @type {HTMLElement} */ (item);
          const text = el.textContent?.toLowerCase() ?? "";
          const match =
            !query || text.includes(query) || subgroupText.includes(query) || groupText.includes(query);
          el.style.display = match ? "" : "none";
          if (match) visibleLeaves += 1;
        });

        const subgroupMatch =
          !query ||
          subgroupText.includes(query) ||
          groupText.includes(query) ||
          visibleLeaves > 0;
        /** @type {HTMLElement} */ (subgroup).style.display = subgroupMatch ? "" : "none";
        if (query && visibleLeaves > 0) {
          subgroup.classList.add("expanded");
          group.classList.add("expanded");
        }
        if (visibleLeaves > 0) visibleChildren += visibleLeaves;
      });

      const groupMatch =
        !query ||
        groupText.includes(query) ||
        visibleChildren > 0;
      /** @type {HTMLElement} */ (group).style.display = groupMatch ? "" : "none";
      if (query && visibleChildren > 0) {
        group.classList.add("expanded");
      }
    });

    navEl.querySelectorAll(".nav-group:not([data-nav-group='modules'])").forEach((group) => {
      const groupText = group.textContent?.toLowerCase() ?? "";
      const childItems = group.querySelectorAll(".nav-tree-item");
      let visibleChildren = 0;

      childItems.forEach((item) => {
        const el = /** @type {HTMLElement} */ (item);
        const text = el.textContent?.toLowerCase() ?? "";
        const match = !query || text.includes(query) || groupText.includes(query);
        el.style.display = match ? "" : "none";
        if (match) visibleChildren += 1;
      });

      const groupMatch =
        !query ||
        groupText.includes(query) ||
        visibleChildren > 0;
      /** @type {HTMLElement} */ (group).style.display = groupMatch ? "" : "none";
      if (query && visibleChildren > 0) {
        group.classList.add("expanded");
      }
    });

    navEl.querySelectorAll(":scope > .nav-item:not(.nav-group-trigger)").forEach((item) => {
      const el = /** @type {HTMLElement} */ (item);
      const text = el.textContent?.toLowerCase() ?? "";
      el.style.display = !query || text.includes(query) ? "" : "none";
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
    badge.title = "Current time";
  };

  tick();
  window.setInterval(tick, 1000);
}

/**
 * @param {HTMLElement | null} badge
 * @param {SidecarRuntimeStats | null | undefined} runtime
 */
function paintLastSyncBadge(badge, runtime) {
  if (!badge) return;

  const unix = runtime?.collectedAtUnix;
  if (unix == null || Number(unix) <= 0) {
    badge.textContent = "Last sync · pending";
    badge.removeAttribute("datetime");
    badge.title = "Waiting for sidecar stats";
    return;
  }

  const formatted = formatLastSync(unix);
  badge.textContent = `Last sync · ${formatted}`;
  badge.setAttribute("datetime", new Date(Number(unix) * 1000).toISOString());
  const modules = runtime?.mountedModules?.length ?? 0;
  badge.title = `${formatted} · ${modules} module${modules === 1 ? "" : "s"} running`;
}

/** @param {SidecarRuntimeStats | null | undefined} [runtime] */
export function refreshLastSyncBadge(runtime = getSidecarRuntime()) {
  paintLastSyncBadge(document.getElementById("last-sync-badge"), runtime);
}

/**
 * @param {HTMLElement | null} el
 * @param {SidecarRuntimeStats | null | undefined} runtime
 */
function paintBrandSubtitle(el, runtime) {
  if (!el) return;
  el.textContent = formatBrandSubtitle(runtime);
  if (runtime?.agentId) {
    el.title = `Agent ${runtime.agentId} · profile ${runtime.sidecarProfile ?? "unknown"}`;
  }
}

/** @param {SidecarRuntimeStats | null | undefined} runtime */
function paintUserChip(runtime) {
  const name = document.querySelector(".user-name");
  if (!name) return;
  name.textContent = runtime?.agentId ? `Node ${runtime.agentId}` : "Node —";
}

/** @param {SidecarRuntimeStats | null | undefined} [runtime] */
export function refreshBrandSubtitle(runtime = getSidecarRuntime()) {
  paintBrandSubtitle(document.getElementById("brand-subtitle"), runtime);
  paintUserChip(runtime);
}

function initBrandSubtitle() {
  const el = document.getElementById("brand-subtitle");
  if (!el) return;

  refreshBrandSubtitle();

  window.addEventListener(SIDECAR_RUNTIME_EVENT, (event) => {
    const detail = /** @type {CustomEvent<SidecarRuntimeStats>} */ (event).detail;
    paintBrandSubtitle(el, detail);
  });

  void loadSidecarRuntime().then((runtime) => {
    paintBrandSubtitle(el, runtime);
  }).catch((error) => {
    el.textContent = formatBrandSubtitleError(error);
    el.title = error instanceof Error ? error.message : "Sidecar stats failed";
  });
}

function initLastSyncBadge() {
  const badge = document.getElementById("last-sync-badge");
  if (!badge) return;

  refreshLastSyncBadge();

  window.addEventListener(SIDECAR_RUNTIME_EVENT, (event) => {
    const detail = /** @type {CustomEvent<SidecarRuntimeStats>} */ (event).detail;
    paintLastSyncBadge(badge, detail);
  });

  void loadSidecarRuntime().then((runtime) => {
    paintLastSyncBadge(badge, runtime);
  });
}
