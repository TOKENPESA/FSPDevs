import { getFaModuleApi } from "../../fa-module-store-api.js";
import {
  mountModuleStoreUi,
  renderModuleStoreShell,
} from "../../../dashboard/dashboard-module-ui.js";
import { escapeHtml, safeUserMessage } from "../../dom-security.js";

export const appStorePanel = {
  id: "fa-app-store",
  title: "Module App Store",
  navLabel: "App Store",
  navIcon: "appStore",
  badge: "hot-swap",
  navDescription:
    "Install, configure, toggle, and uninstall FA edge modules without restarting the sidecar.",
  render() {
    return renderModuleStoreShell({
      catalogTitle: "Edge module catalog",
      catalogHint:
        "Native sidecar modules (DICOBA, fiat bridge, etc.) ready for runtime hot-mount.",
      installedTitle: "Mounted modules",
      installedHint:
        "Synchronized with the sidecar DynamicModuleRegistry — no restart required.",
      entityLabel: "module",
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
      entityLabel: "module",
      catalogTitle: "Edge module catalog",
      installedTitle: "Mounted modules",
    });
  },
};
