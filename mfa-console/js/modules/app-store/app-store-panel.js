import { getMfaModuleApi } from "../../mfa-module-store-api.js";
import {
  mountModuleStoreUi,
  renderModuleStoreShell,
} from "../../../../dashboard/dashboard-module-ui.js";
import { escapeHtml, safeUserMessage } from "../../dom-security.js";

export const appStorePanel = {
  id: "mfa-app-store",
  title: "Policy App Store",
  navLabel: "App Store",
  navIcon: "appStore",
  badge: "hot-swap",
  navDescription:
    "Install, configure, toggle, and uninstall MFA policy plugins without restarting the supervisor.",
  render() {
    return renderModuleStoreShell({
      catalogTitle: "Policy catalog",
      catalogHint:
        "Supervisor plugins (routing, compliance, clearing) available for runtime mount.",
      installedTitle: "Mounted plugins",
      installedHint:
        "Reflects the live plugin registry — toggles apply immediately to routing and compliance paths.",
      entityLabel: "plugin",
    });
  },
  /**
   * @param {HTMLElement} root
   */
  mount(root) {
    mountModuleStoreUi(root, {
      api: getMfaModuleApi(),
      escapeHtml,
      safeUserMessage,
      scope: "mfa-app-store",
      entityLabel: "plugin",
      catalogTitle: "Policy catalog",
      installedTitle: "Mounted plugins",
    });
  },
};
