import { eventsPanel } from "./events-panel.js";

export default {
  id: "events",
  label: "Event Log",
  navLabel: "Events",
  navIcon: "events",
  navDescription: "Live monitor stream and supervisor actions",
  panels: [eventsPanel],
};
