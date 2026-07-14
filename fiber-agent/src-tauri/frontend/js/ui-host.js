import { initNavSearch, initShell, closeMobileSidebar, refreshLastSyncBadge, refreshBrandSubtitle } from "./shell.js";
import {
  bindDashboardActions,
  loadDashboardSnapshot,
  renderDashboardStats,
} from "./dashboard-stats.js";
import { mountOobFallbackCard } from "./oob-fallback.js";
import { renderModulePage } from "./panel-shell.js";
import { loadSidecarRuntime } from "./sidecar-runtime.js";
import { icon, navIcon } from "./icons.js";
import appStoreModule from "./modules/app-store/index.js";
import fundingModule from "./modules/funding/index.js";
import { modulesForMounted } from "./module-registry.js";

/** @typedef {import("../../../../dashboard/types.js").SidecarModule} SidecarModule */
/** @typedef {import("../../../../dashboard/types.js").SidecarPanel} SidecarPanel */
/** @typedef {import("../../../../dashboard/types.js").SidecarUiContext} SidecarUiContext */
/** @typedef {import("../../../../dashboard/types.js").PanelRoute} PanelRoute */
/** @typedef {import("../../../../dashboard/types.js").ModuleNavGroup} ModuleNavGroup */

const MODULES_ROOT = {
  id: "modules",
  label: "Module",
  icon: "modules",
  hint: "Mounted sidecar modules",
};

/**
 * @param {SidecarModule} sidecarModule
 * @param {SidecarPanel} panel
 * @returns {PanelRoute}
 */
function panelRoute(sidecarModule, panel) {
  const label = panel.navLabel ?? panel.title;
  return {
    id: panel.id,
    type: "panel",
    label,
    icon: panel.navIcon ?? "modules",
    hint: panel.navDescription ?? sidecarModule.navDescription ?? label,
    panel,
    module: sidecarModule,
  };
}

export class SidecarUiHost {
  /**
   * @param {HTMLElement | null} contentEl
   * @param {HTMLElement | null} navEl
   */
  constructor(contentEl, navEl) {
    this.contentEl = contentEl;
    this.navEl = navEl;
    /** @type {SidecarModule[]} */
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
    /** @type {SidecarUiContext} */
    this.ctx = { root: /** @type {HTMLElement} */ (contentEl) };
  }

  /** @param {SidecarModule} sidecarModule */
  ensureRegistered(sidecarModule) {
    if (this.modules.some((entry) => entry.id === sidecarModule.id)) {
      return;
    }
    this.register(sidecarModule);
  }

  /** @param {SidecarModule} sidecarModule */
  register(sidecarModule) {
    if (this.modules.some((entry) => entry.id === sidecarModule.id)) {
      return;
    }
    this.modules.push(sidecarModule);
    const childRoutes = sidecarModule.panels.map((panel) =>
      panelRoute(sidecarModule, panel),
    );
    this.panelRoutes.push(...childRoutes);

    if (sidecarModule.topLevel) {
      this.topLevelRoutes.push(...childRoutes);
      return;
    }

    this.moduleGroups.push({
      id: sidecarModule.id,
      label: sidecarModule.navLabel ?? sidecarModule.label,
      icon: sidecarModule.navIcon ?? "modules",
      children: childRoutes,
    });
  }

  /** @param {string} moduleId */
  unregisterModule(moduleId) {
    const sidecarModule = this.modules.find((entry) => entry.id === moduleId);
    if (!sidecarModule || sidecarModule.topLevel) {
      return;
    }

    const removedRouteIds = new Set(sidecarModule.panels.map((panel) => panel.id));
    this.modules = this.modules.filter((entry) => entry.id !== moduleId);
    this.panelRoutes = this.panelRoutes.filter((route) => !removedRouteIds.has(route.id));
    this.moduleGroups = this.moduleGroups.filter((group) => group.id !== moduleId);
    this.initializedModules.delete(moduleId);
    this.expandedGroups.delete(moduleId);
  }

  /**
   * Keep sidebar module panels aligned with actively running backend modules.
   * @param {string[]} mountedIds
   */
  syncRunningModules(mountedIds) {
    const runningIds = new Set(mountedIds);
    const desiredModules = modulesForMounted(mountedIds);

    for (const sidecarModule of this.modules) {
      if (sidecarModule.topLevel) continue;
      if (!runningIds.has(sidecarModule.id)) {
        this.unregisterModule(sidecarModule.id);
      }
    }

    for (const sidecarModule of desiredModules) {
      this.register(sidecarModule);
    }

    const activeRouteIds = new Set(this.allRoutes().map((route) => route.id));
    if (!activeRouteIds.has(this.currentRoute)) {
      this.currentRoute = "dashboard";
    }

    this.renderNav();
  }

  /** @returns {PanelRoute[]} */
  allRoutes() {
    return [
      {
        id: "dashboard",
        type: "dashboard",
        label: "Dashboard",
        icon: "dashboard",
        hint: "Sidecar summary and stats",
      },
      ...this.panelRoutes,
    ];
  }

  /** @param {string} routeId @returns {ModuleNavGroup | undefined} */
  findModuleGroupForRoute(routeId) {
    return this.moduleGroups.find((group) =>
      group.children.some((child) => child.id === routeId),
    );
  }

  /** @param {ModuleNavGroup} group @returns {boolean} */
  isGroupActive(group) {
    return group.children.some((child) => child.id === this.currentRoute);
  }

  refreshLoanPanel() {
    const loanRoute = this.panelRoutes.find((route) => route.id === "dicoba-loan");
    return loanRoute?.panel?.refresh?.(this.ctx.root);
  }

  /** @param {PanelRoute} route @returns {string} */
  renderNavTreeItem(route) {
    return `
      <button
        type="button"
        class="nav-tree-item${route.id === this.currentRoute ? " active" : ""}"
        data-route-id="${route.id}"
        title="${route.label}"
      >
        ${navIcon(route.icon, 14)}
        <span class="nav-tree-label">${route.label}</span>
      </button>
    `;
  }

  /** @param {PanelRoute} route @returns {string} */
  renderNavEntry(route) {
    return `
      <button
        type="button"
        class="nav-item${route.id === this.currentRoute ? " active" : ""}"
        data-route-id="${route.id}"
        title="${route.hint ?? route.label}"
      >
        <span class="nav-icon-box">${navIcon(route.icon, 16)}</span>
        <span class="nav-label">${route.label}</span>
      </button>
    `;
  }

  /**
   * @param {ModuleNavGroup} group
   * @param {boolean} isExpanded
   * @param {boolean} [childActive]
   * @returns {string}
   */
  renderNavGroupTrigger(group, isExpanded, childActive = this.isGroupActive(group)) {
    return `
      <button
        type="button"
        class="nav-item nav-group-trigger${childActive ? " active" : ""}"
        data-toggle-group="${group.id}"
        aria-expanded="${isExpanded}"
        title="${group.hint ?? group.label}"
      >
        <span class="nav-icon-box">${navIcon(group.icon, 16)}</span>
        <span class="nav-label">${group.label}</span>
        <span class="nav-chevron">${icon(isExpanded ? "chevronDown" : "chevronRight", 14)}</span>
      </button>
    `;
  }

  /**
   * @param {ModuleNavGroup} moduleGroup
   * @param {boolean} isExpanded
   * @returns {string}
   */
  renderNavSubgroup(moduleGroup, isExpanded) {
    const childActive = this.isGroupActive(moduleGroup);

    return `
      <div class="nav-subgroup${isExpanded ? " expanded" : ""}" data-nav-group="${moduleGroup.id}">
        <button
          type="button"
          class="nav-subgroup-trigger${childActive ? " active" : ""}"
          data-toggle-group="${moduleGroup.id}"
          aria-expanded="${isExpanded}"
          title="${moduleGroup.label}"
        >
          <span class="nav-subgroup-icon">${navIcon(moduleGroup.icon, 14)}</span>
          <span class="nav-subgroup-label">${moduleGroup.label}</span>
          <span class="nav-sub-chevron">${icon(isExpanded ? "chevronDown" : "chevronRight", 12)}</span>
        </button>
        <div class="nav-subtree">
          ${moduleGroup.children.map((child) => this.renderNavTreeItem(child)).join("")}
        </div>
      </div>
    `;
  }

  /** @returns {string} */
  renderModulesRoot() {
    const isExpanded =
      this.expandedGroups.has(MODULES_ROOT.id) ||
      this.moduleGroups.some((group) => this.isGroupActive(group));

    const rootGroup = /** @type {ModuleNavGroup} */ ({
      ...MODULES_ROOT,
      children: /** @type {PanelRoute[]} */ (/** @type {unknown} */ (this.moduleGroups)),
    });
    const childActive = this.moduleGroups.some((group) =>
      this.isGroupActive(group),
    );

    return `
      <div class="nav-group${isExpanded ? " expanded" : ""}" data-nav-group="${MODULES_ROOT.id}">
        ${this.renderNavGroupTrigger(rootGroup, isExpanded, childActive)}
        <div class="nav-tree">
          ${this.moduleGroups
            .map((moduleGroup) => {
              const subgroupExpanded =
                this.expandedGroups.has(moduleGroup.id) ||
                this.isGroupActive(moduleGroup);
              return this.renderNavSubgroup(moduleGroup, subgroupExpanded);
            })
            .join("")}
        </div>
      </div>
    `;
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
      button.addEventListener("click", () => {
        const routeId = /** @type {HTMLButtonElement} */ (button).dataset.routeId;
        if (routeId) void this.navigate(routeId);
      });
    });

    this.navEl.querySelectorAll("[data-toggle-group]").forEach((button) => {
      button.addEventListener("click", () => {
        const groupId = /** @type {HTMLButtonElement} */ (button).dataset.toggleGroup;
        if (!groupId) return;
        if (this.expandedGroups.has(groupId)) {
          this.expandedGroups.delete(groupId);
        } else {
          this.expandedGroups.add(groupId);
        }
        this.renderNav();
      });
    });
  }

  /** @param {string} routeId */
  expandGroupsForRoute(routeId) {
    if (routeId !== "dashboard") {
      this.expandedGroups.add(MODULES_ROOT.id);
    }
    const parentGroup = this.findModuleGroupForRoute(routeId);
    if (parentGroup) {
      this.expandedGroups.add(parentGroup.id);
    }
  }

  renderDashboard() {
    if (!this.contentEl) return;
    this.contentEl.innerHTML = `
      <div class="dashboard-loading" data-dashboard-loading>Loading sidecar stats…</div>
    `;

    void this.paintDashboard();
  }

  async paintDashboard() {
    if (!this.contentEl) return;
    const runtime = await loadDashboardSnapshot();
    const runningModules = Array.isArray(runtime?.mountedModules)
      ? runtime.mountedModules
      : [];
    this.syncRunningModules(runningModules);
    this.contentEl.innerHTML = renderDashboardStats(runtime);
    refreshLastSyncBadge(runtime);
    refreshBrandSubtitle(runtime);

    bindDashboardActions(this.contentEl, {
      onRefresh: () => {
        if (!this.contentEl) return;
        this.contentEl.innerHTML = `
          <div class="dashboard-loading" data-dashboard-loading>Loading sidecar stats…</div>
        `;
        void this.paintDashboard();
      },
      onNavigate: (routeId) => {
        void this.navigate(routeId);
      },
    });

    await mountOobFallbackCard(this.contentEl);
  }

  /** @param {PanelRoute} route */
  async renderPanel(route) {
    if (!this.contentEl || !route.panel) return;
    const panel = route.panel;

    this.contentEl.innerHTML = renderModulePage({
      panelId: panel.id,
      title: panel.title,
      description: panel.navDescription ?? "",
      icon: panel.navIcon ?? route.icon ?? "modules",
      badge: panel.badge ?? route.module?.id ?? "",
      contentHtml: panel.render(),
    });

    this.ctx.refreshLoanPanel = () => this.refreshLoanPanel();
    const mountResult = panel.mount(this.ctx.root, this.ctx);
    if (mountResult instanceof Promise) {
      await mountResult;
    }

    if (route.module?.initialize && !this.initializedModules.has(route.module.id)) {
      this.initializedModules.add(route.module.id);
      await route.module.initialize(this.ctx);
    } else if (route.id === "dicoba-loan") {
      await panel.refresh?.(this.ctx.root);
    }
  }

  /** @param {string} routeId */
  async navigate(routeId) {
    const route = this.allRoutes().find((entry) => entry.id === routeId);
    if (!route) return;

    this.currentRoute = routeId;
    this.expandGroupsForRoute(routeId);
    this.renderNav();
    closeMobileSidebar();

    if (route.type === "dashboard") {
      this.renderDashboard();
      return;
    }

    await loadSidecarRuntime({ force: true });
    refreshLastSyncBadge();
    refreshBrandSubtitle();
    await this.renderPanel(route);
  }

  async boot() {
    this.ensureRegistered(fundingModule);
    this.ensureRegistered(appStoreModule);
    initShell();
    if (this.navEl) initNavSearch(this.navEl);
    this.ctx.refreshLoanPanel = () => this.refreshLoanPanel();
    this.expandedGroups.add(MODULES_ROOT.id);
    for (const group of this.moduleGroups) {
      this.expandedGroups.add(group.id);
    }
    this.renderNav();
    await this.navigate("dashboard");
  }
}
