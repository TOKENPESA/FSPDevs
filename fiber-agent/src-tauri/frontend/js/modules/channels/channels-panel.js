import { channelsPanelMarkup, mountChannelsPanel } from "../../channels.js";

/** @typedef {import("../../../../../../dashboard/types.js").SidecarPanel} SidecarPanel */

/** @type {SidecarPanel} */
export const channelsPanel = {
  id: "fa-channels",
  title: "Channels",
  navLabel: "Channels",
  navIcon: "channels",
  badge: "Mesh",
  navDescription: "Open or close Fiber channels with agents discovered on MFA",
  render() {
    return channelsPanelMarkup();
  },
  /**
   * @param {HTMLElement} root
   */
  async mount(root) {
    await mountChannelsPanel(root);
  },
};
