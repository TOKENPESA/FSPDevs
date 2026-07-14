import { getFaModuleApi } from "../../fa-module-store-api.js";
import {
  mountModuleStoreUi,
  renderModuleStoreShell,
} from "../../../dashboard/dashboard-module-ui.js";
import { escapeHtml, safeUserMessage } from "../../dom-security.js";

export const appStorePanel = {
  id: "fa-app-store",
  title: "App Store",
  navLabel: "App Store",
  navIcon: "appStore",
  badge: "Tools",
  navDescription: "Install, turn on/off, or remove tools without restarting.",
  render() {
    return renderModuleStoreShell({
      catalogTitle: "Available tools",
      catalogHint: "Tools you can add to this agent.",
      installedTitle: "Installed tools",
      installedHint: "Changes apply immediately — no restart needed.",
      entityLabel: "tool",
    });
  },
  /**
   * @param {HTMLElement} root
   */
  mount(root) {
    mountModuleStoreUi(root, {
      api: getFaModuleApi(),
      escapeHtml,
      safeUserMessage,
      scope: "fa-app-store",
      entityLabel: "tool",
      catalogTitle: "Available tools",
      installedTitle: "Installed tools",
    });
  },
};
