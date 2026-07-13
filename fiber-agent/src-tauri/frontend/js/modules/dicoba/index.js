import { loanPanel } from "./loan-panel.js";
import { savingsPanel } from "./savings-panel.js";
import { syncVaultTransparency } from "./state.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarModule} SidecarModule */
/** @typedef {import("../../../../../../dashboard/types.js").SidecarUiContext} SidecarUiContext */

/** @type {SidecarModule} */
export default {
  id: "dicoba",
  label: "DICOBA JunguKuu",
  navLabel: "DICOBA",
  navIcon: "dicoba",
  navDescription:
    "Collective savings, loans, and JunguKuu vault operations",
  panels: [savingsPanel, loanPanel],
  /** @param {SidecarUiContext} ctx */
  async initialize(ctx) {
    await syncVaultTransparency();
    await loanPanel.refresh?.(ctx.root);
  },
};
