import { fundingPanelMarkup, mountFundingPanel } from "../../funding.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarPanel} SidecarPanel */

/** @type {SidecarPanel} */
export const fundingPanel = {
  id: "fa-funding",
  title: "Add funds",
  navLabel: "Add funds",
  navIcon: "funding",
  badge: "Setup",
  navDescription:
    "Use JoyID or the faucet to add test coins so you can open payment links",
  render() {
    return fundingPanelMarkup();
  },
  /**
   * @param {HTMLElement} root
   */
  async mount(root) {
    await mountFundingPanel(root);
  },
};
