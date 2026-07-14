import { loanPanel } from "./loan-panel.js";
import { savingsPanel } from "./savings-panel.js";
import { syncVaultTransparency } from "./state.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarModule} SidecarModule */
/** @typedef {import("../../../../../../dashboard/types.js").SidecarUiContext} SidecarUiContext */

/** @type {SidecarModule} */
export default {
  id: "dicoba",
  label: "DiCoBa savings groups",
  navLabel: "DiCoBa",
  navIcon: "dicoba",
  navDescription: "Group savings, loans, and shared funds",
  panels: [savingsPanel, loanPanel],
  /** @param {SidecarUiContext} ctx */
  async initialize(ctx) {
    await syncVaultTransparency();
    await loanPanel.refresh?.(ctx.root);
  },
};
