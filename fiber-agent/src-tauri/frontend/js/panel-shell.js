import { icon } from "./icons.js";

/**
 * @param {Object} options
 * @param {string} options.panelId
 * @param {string} options.title
 * @param {string} [options.description]
 * @param {import("./icons.js").IconName | string} [options.icon]
 * @param {string} [options.badge]
 * @param {string} [options.contentHtml]
 * @returns {string}
 */
export function renderModulePage({
  panelId,
  title,
  description = "",
  icon: iconKey = "modules",
  badge = "",
  contentHtml = "",
}) {
  return `
    <section class="dashboard-page module-page" data-module-page="${panelId}">
      <header class="dashboard-hero">
        <div class="dashboard-hero-copy">
          ${badge ? `<span class="module-badge module-badge-hero">${badge}</span>` : ""}
          <h1>${title}</h1>
          ${description ? `<p>${description}</p>` : ""}
        </div>
        <div class="dashboard-hero-visual" aria-hidden="true">
          <div class="hero-node-stack">
            <div class="hero-node hero-node-main">${icon(iconKey, 28)}</div>
            <div class="hero-node hero-node-orbit">${icon("modules", 18)}</div>
            <div class="hero-node hero-node-orbit delay">${icon("dashboard", 18)}</div>
          </div>
        </div>
      </header>

      <div class="dashboard-body module-body">
        <div class="module-workspace">
          ${contentHtml}
        </div>
        <aside class="dashboard-aside module-aside">
          <div data-module-stats-host></div>
        </aside>
      </div>
    </section>
  `;
}
