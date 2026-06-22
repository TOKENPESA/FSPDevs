export const MFA_HEALTH_URL = "http://127.0.0.1:1025/";
export const MFA_SIMULATION_URL = "http://127.0.0.1:1025/simulation";
export const MFA_ROUTE_URL = "http://127.0.0.1:1025/route";
export const EDGE_NODES_STORAGE_KEY = "tpxdevs-edge-nodes";
export const RING_MAX = 1024;
export const COMM_TTL_MS = 30_000;
export const PAYMENT_TRAVEL_CAP = 0.92;
export const PAYMENT_SETTLE_DISPLAY_MS = 8_000;

export const MFA_HUB = { x: 52, y: 52 };

export const PATH_STYLES = {
  ring: { color: "#50ff9a", width: 2.8, dash: [7, 5], speed: 0.004, label: "Ring +1" },
  skip: { color: "#5eb5ff", width: 2.8, dash: [9, 6], speed: 0.005, label: "Skip +2" },
  chord: { color: "#c678ff", width: 2.8, dash: [5, 7], speed: 0.003, label: "Opposite" },
  mfa: { color: "#ffb347", width: 2.2, dash: [11, 9], speed: 0.006, label: "MFA uplink" },
};

export const COMM_STYLE = {
  mesh: { color: "#00e5ff", width: 2.4, dash: [6, 4], speed: 0.007 },
  heal: { color: "#00d4ff", width: 3, dash: [4, 3], speed: 0.009 },
  mfa: PATH_STYLES.mfa,
};
