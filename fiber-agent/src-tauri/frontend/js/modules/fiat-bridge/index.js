import { feePanel } from "./fee-panel.js";
import { floatPanel } from "./float-panel.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarModule} SidecarModule */

/** @type {SidecarModule} */
export default {
  id: "fiat_bridge",
  label: "Mobile money",
  navLabel: "Mobile money",
  navIcon: "mobile",
  navDescription: "Cash reserves, customer deposits, and fee preview",
  panels: [floatPanel, feePanel],
};
