import { appStorePanel } from "./app-store-panel.js";

export default {
  id: "app-store",
  label: "App Store",
  navLabel: "App Store",
  navIcon: "appStore",
  navDescription: "Hot-swap MFA policy plugins at runtime",
  topLevel: true,
  panels: [appStorePanel],
};
