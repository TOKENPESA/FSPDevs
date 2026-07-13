import { routePanel } from "./route-panel.js";

export default {
  id: "routing",
  label: "Routing & Payments",
  navLabel: "Routing",
  navIcon: "routing",
  navDescription: "Multi-hop route discovery and mesh payment execution",
  panels: [routePanel],
};
