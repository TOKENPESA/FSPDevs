import { initNavSearch, initShell, closeMobileSidebar, refreshLastSyncBadge } from "./shell.js";
import {
  bindDashboardActions,
  loadDashboardSnapshot,
  renderDashboardStats,
} from "./dashboard-stats.js";
import { renderModulePage } from "./panel-shell.js";
import { loadMfaRuntime, MFA_RUNTIME_EVENT, startMfaRuntimeWatcher } from "./mfa-runtime.js";
import { icon, navIcon } from "./icons.js";
import appStoreModule from "./modules/app-store/index.js";
import { parkMeshCanvas } from "./mesh-canvas.js";

/** @typedef {import('./types.js').MfaModule} MfaModule */
/** @typedef {import('./types.js').MfaPanel} MfaPanel */
/** @typedef {import('./types.js').MfaUiHostContext} MfaUiHostContext */
/** @typedef {import('./types.js').PanelRoute} PanelRoute */
/** @typedef {import('./types.js').ModuleNavGroup} ModuleNavGroup */
/** @typedef {import('./types.js').MfaRuntimeDetail} MfaRuntimeDetail */

const MODULES_ROOT = {
  id: "supervisor",
  label: "Supervisor",
  icon: "modules",
  hint: "MFA control plane modules",
};

/**
 * @param {MfaModule} mfaModule
 * @param {MfaPanel} panel
 * @returns {PanelRoute}
 */
function panelRoute(mfaModule, panel) {
  return {
    id: panel.id,
    type: "panel",
    label: panel.navLabel ?? panel.title,
    icon: panel.navIcon ?? "modules",
    panel,
    module: mfaModule,
  };
}

export class MfaUiHost {
  /**
   * @param {HTMLElement} contentEl
   * @param {HTMLElement} navEl
   */
  constructor(contentEl, navEl) {
    this.contentEl = contentEl;
    this.navEl = navEl;
    /** @type {MfaModule[]} */
    this.modules = [];
    /** @type {PanelRoute[]} */
    this.panelRoutes = [];
    /** @type {ModuleNavGroup[]} */
    this.moduleGroups = [];
    /** @type {PanelRoute[]} */
    this.topLevelRoutes = [];
    this.currentRoute = "dashboard";
    this.expandedGroups = new Set();
    this.initializedModules = new Set();
    /** @type {MfaUiHostContext} */
    this.ctx = { root: contentEl };
  }

  /**
   * @param {MfaModule} mfaModule
   */
  ensureRegistered(mfaModule) {
    if (this.modules.some((entry) => entry.id === mfaModule.id)) {
      return;
    }
    this.register(mfaModule);
  }

  /**
   * @param {MfaModule} mfaModule
   */
  register(mfaModule) {
    if (this.modules.some((entry) => entry.id === mfaModule.id)) {
      return;
    }
    this.modules.push(mfaModule);
    const childRoutes = mfaModule.panels.map((panel) => panelRoute(mfaModule, panel));
    this.panelRoutes.push(...childRoutes);

    if (mfaModule.topLevel) {
      this.topLevelRoutes.push(...childRoutes);
      return;
    }

    this.moduleGroups.push({
      id: mfaModule.id,
      label: mfaModule.navLabel ?? mfaModule.label,
      icon: mfaModule.navIcon ?? "modules",
      children: childRoutes,
    });
  }

  /** @returns {PanelRoute[]} */
  allRoutes() {
    return [
      {
        id: "dashboard",
        type: "dashboard",
        label: "Dashboard",
        icon: "dashboard",
        hint: "MFA supervisor summary",
      },
      ...this.panelRoutes,
    ];
  }

  /**
   * @param {string} routeId
   * @returns {ModuleNavGroup | undefined}
   */
  findModuleGroupForRoute(routeId) {
    return this.moduleGroups.find((group) =>
      group.children.some((child) => child.id === routeId),
    );
  }

  /**
   * @param {ModuleNavGroup} group
   */
  isGroupActive(group) {
    return group.children.some((child) => child.id === this.currentRoute);
  }

  /**
   * @param {PanelRoute} route
   */
  renderNavTreeItem(route) {
    return `
      <button type="button" class="nav-tree-item${route.id === this.currentRoute ? " active" : ""}"
        data-route-id="${route.id}" title="${route.label}">
        ${navIcon(route.icon, 14)}
        <span class="nav-tree-label">${route.label}</span>
      </button>`;
  }

  /**
   * @param {PanelRoute} route
   */
  renderNavEntry(route) {
    return `
      <button type="button" class="nav-item${route.id === this.currentRoute ? " active" : ""}"
        data-route-id="${route.id}" title="${route.hint ?? route.label}">
        <span class="nav-icon-box">${navIcon(route.icon, 16)}</span>
        <span class="nav-label">${route.label}</span>
      </button>`;
  }

  /**
   * @param {ModuleNavGroup} group
   * @param {boolean} isExpanded
   * @param {boolean} [childActive]
   */
  renderNavGroupTrigger(group, isExpanded, childActive = this.isGroupActive(group)) {
    return `
      <button type="button" class="nav-item nav-group-trigger${childActive ? " active" : ""}"
        data-toggle-group="${group.id}" aria-expanded="${isExpanded}" title="${group.hint ?? group.label}">
        <span class="nav-icon-box">${navIcon(group.icon, 16)}</span>
        <span class="nav-label">${group.label}</span>
        <span class="nav-chevron">${icon(isExpanded ? "chevronDown" : "chevronRight", 14)}</span>
      </button>`;
  }

  /**
   * @param {ModuleNavGroup} moduleGroup
   * @param {boolean} isExpanded
   */
  renderNavSubgroup(moduleGroup, isExpanded) {
    const childActive = this.isGroupActive(moduleGroup);
    return `
      <div class="nav-subgroup${isExpanded ? " expanded" : ""}" data-nav-group="${moduleGroup.id}">
        <button type="button" class="nav-subgroup-trigger${childActive ? " active" : ""}"
          data-toggle-group="${moduleGroup.id}" aria-expanded="${isExpanded}" title="${moduleGroup.label}">
          <span class="nav-subgroup-icon">${navIcon(moduleGroup.icon, 14)}</span>
          <span class="nav-subgroup-label">${moduleGroup.label}</span>
          <span class="nav-sub-chevron">${icon(isExpanded ? "chevronDown" : "chevronRight", 12)}</span>
        </button>
        <div class="nav-subtree">
          ${moduleGroup.children.map((child) => this.renderNavTreeItem(child)).join("")}
        </div>
      </div>`;
  }

  renderModulesRoot() {
    const isExpanded =
      this.expandedGroups.has(MODULES_ROOT.id) ||
      this.moduleGroups.some((group) => this.isGroupActive(group));
    /** @type {ModuleNavGroup} */
    const rootGroup = { ...MODULES_ROOT, children: [] };
    const childActive = this.moduleGroups.some((group) => this.isGroupActive(group));
    return `
      <div class="nav-group${isExpanded ? " expanded" : ""}" data-nav-group="${MODULES_ROOT.id}">
        ${this.renderNavGroupTrigger(rootGroup, isExpanded, childActive)}
        <div class="nav-tree">
          ${this.moduleGroups
            .map((moduleGroup) => {
              const subgroupExpanded =
                this.expandedGroups.has(moduleGroup.id) || this.isGroupActive(moduleGroup);
              return this.renderNavSubgroup(moduleGroup, subgroupExpanded);
            })
            .join("")}
        </div>
      </div>`;
  }

  renderNav() {
    if (!this.navEl) return;
    const dashboard = this.allRoutes()[0];
    const dashboardHtml = this.renderNavEntry(dashboard);
    const topLevelHtml = this.topLevelRoutes
      .map((route) => this.renderNavEntry(route))
      .join("");
    const modulesHtml =
      this.moduleGroups.length > 0 ? this.renderModulesRoot() : "";

    this.navEl.innerHTML = dashboardHtml + topLevelHtml + modulesHtml;

    this.navEl.querySelectorAll("[data-route-id]").forEach((button) => {
      if (!(button instanceof HTMLButtonElement)) return;
      button.addEventListener("click", () => void this.navigate(button.dataset.routeId ?? ""));
    });
    this.navEl.querySelectorAll("[data-toggle-group]").forEach((button) => {
      if (!(button instanceof HTMLButtonElement)) return;
      button.addEventListener("click", () => {
        const groupId = button.dataset.toggleGroup;
        if (!groupId) return;
        if (this.expandedGroups.has(groupId)) this.expandedGroups.delete(groupId);
        else this.expandedGroups.add(groupId);
        this.renderNav();
      });
    });
  }

  /**
   * @param {string} routeId
   */
  expandGroupsForRoute(routeId) {
    if (routeId !== "dashboard") this.expandedGroups.add(MODULES_ROOT.id);
    const parentGroup = this.findModuleGroupForRoute(routeId);
    if (parentGroup) this.expandedGroups.add(parentGroup.id);
  }

  async renderDashboard() {
    parkMeshCanvas();
    this.contentEl.innerHTML = `<div class="dashboard-loading">Loading MFA stats…</div>`;
    const runtime = await loadDashboardSnapshot();
    this.paintDashboard(runtime);
  }

  /**
   * @param {MfaRuntimeDetail | null} runtime
   */
  paintDashboard(runtime) {
    parkMeshCanvas();
    this.contentEl.innerHTML = renderDashboardStats(runtime);
    refreshLastSyncBadge(runtime ?? undefined);
    bindDashboardActions(this.contentEl, {
      onRefresh: () => void this.renderDashboard(),
      onConnect: () => this.ctx.connectMonitor?.(),
      onNavigate: (routeId) => {
        void this.navigate(routeId);
      },
    });
  }

  /**
   * @param {PanelRoute} route
   */
  async renderPanel(route) {
    const panel = route.panel;
    if (!panel || !route.module) return;

    parkMeshCanvas();
    this.contentEl.innerHTML = renderModulePage({
      panelId: panel.id,
      title: panel.title,
      description: panel.navDescription ?? "",
      icon: panel.navIcon ?? route.icon ?? "modules",
      badge: panel.badge ?? route.module.id,
      contentHtml: panel.render(),
    });

    const mountEl = this.contentEl.querySelector(".module-workspace");
    if (!mountEl || !(mountEl instanceof HTMLElement)) return;

    const mountResult = panel.mount(mountEl, this.ctx);
    if (mountResult instanceof Promise) await mountResult;

    const statsHost = this.contentEl.querySelector("[data-module-stats-host]");
    if (statsHost && panel.renderAside) {
      statsHost.innerHTML = panel.renderAside(this.ctx);
    }

    if (route.module.initialize && !this.initializedModules.has(route.module.id)) {
      this.initializedModules.add(route.module.id);
      await route.module.initialize(this.ctx);
    }
  }

  /**
   * @param {string} routeId
   */
  async navigate(routeId) {
    const route = this.allRoutes().find((entry) => entry.id === routeId);
    if (!route) return;
    // Always detach canvas before any panel swap so it cannot linger on other routes.
    parkMeshCanvas();
    this.currentRoute = routeId;
    this.expandGroupsForRoute(routeId);
    this.renderNav();
    closeMobileSidebar();

    if (route.type === "dashboard") {
      await this.renderDashboard();
      return;
    }

    await loadMfaRuntime({ force: true });
    refreshLastSyncBadge();
    await this.renderPanel(route);
  }

  async boot() {
    this.ensureRegistered(appStoreModule);
    initShell();
    initNavSearch(this.navEl);
    startMfaRuntimeWatcher();
    window.addEventListener(MFA_RUNTIME_EVENT, (event) => {
      if (this.currentRoute !== "dashboard") return;
      const detail = /** @type {CustomEvent<MfaRuntimeDetail | null>} */ (event).detail;
      this.paintDashboard(detail);
    });
    this.expandedGroups.add(MODULES_ROOT.id);
    for (const group of this.moduleGroups) this.expandedGroups.add(group.id);
    this.renderNav();
    await this.navigate("dashboard");
  }
}
