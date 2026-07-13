import { appStorePanel } from "./app-store-panel.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarModule} SidecarModule */

/** @type {SidecarModule} */
export default {
  id: "app-store",
  label: "App Store",
  navLabel: "App Store",
  navIcon: "appStore",
  navDescription: "Hot-swap FA edge modules at runtime",
  topLevel: true,
  panels: [appStorePanel],
};
