import { fundingPanel } from "./funding-panel.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarModule} SidecarModule */

/** @type {SidecarModule} */
export default {
  id: "funding",
  label: "Global Funding Onboarding",
  navLabel: "Funding",
  navIcon: "funding",
  navDescription: "Faucet and JoyID funding for the local FNN node",
  topLevel: true,
  panels: [fundingPanel],
};
