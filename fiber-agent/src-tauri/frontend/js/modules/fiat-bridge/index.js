import { feePanel } from "./fee-panel.js";
import { floatPanel } from "./float-panel.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarModule} SidecarModule */

/** @type {SidecarModule} */
export default {
  id: "fiat_bridge",
  label: "Mobile Money Float Bridge",
  navLabel: "Mobile Money",
  navIcon: "mobile",
  navDescription:
    "Telco float bridge, cash-in routing, and fee estimation",
  panels: [floatPanel, feePanel],
};
