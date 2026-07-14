import { fundingPanelMarkup, mountFundingPanel } from "../../funding.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarPanel} SidecarPanel */

/** @type {SidecarPanel} */
export const fundingPanel = {
  id: "fa-funding",
  title: "Global Funding Onboarding",
  navLabel: "Funding",
  navIcon: "funding",
  badge: "phase-e",
  navDescription:
    "Fund the local FNN with Nervos faucet CKB or JoyID passkey via CCC",
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
