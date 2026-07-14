import { fundingPanel } from "./funding-panel.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarModule} SidecarModule */

/** @type {SidecarModule} */
export default {
  id: "funding",
  label: "Add funds",
  navLabel: "Add funds",
  navIcon: "funding",
  navDescription: "Get test funds or send coins to this device",
  topLevel: true,
  panels: [fundingPanel],
};
