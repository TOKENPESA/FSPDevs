import { channelsPanel } from "./channels-panel.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarModule} SidecarModule */

/** @type {SidecarModule} */
export default {
  id: "channels",
  label: "Channels",
  navLabel: "Channels",
  navIcon: "channels",
  navDescription: "Open or close Fiber channels with agents discovered on MFA",
  topLevel: true,
  panels: [channelsPanel],
};
