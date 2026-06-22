import { COMM_TTL_MS } from "./config.js";

export const state = {
  networkSize: 1024,
  dead: new Set(),
  healed: new Set(),
  healLinks: [],
  activeRoute: [],
  /** One-shot payment animation: source → destination, stops at dest when settled. */
  paymentTransfer: null,
  tick: 0,
  healCount: 0,
  playing: true,
  speed: 1,
  ws: null,
  hoveredNode: null,
  lastPointer: null,
  dirty: true,
  lastFrame: 0,
  animTime: 0,
  hub: {
    rpcUrl: "—",
    fundingShannons: "—",
    sidecarAlerts: "off (single-node)",
  },
  liquidity: {
    injections: 0,
    faucetHints: 0,
    inFlight: 0,
    failed: 0,
    lastEvent: "—",
    byNode: new Map(),
  },
  /** Nodes and edges with recent MFA telemetry (heartbeats / heals). */
  comm: {
    nodes: new Map(),
    edges: new Map(),
    mfaLinks: new Map(),
    /** Persistent per-FA balances (survives comm TTL / heartbeat peer refresh). */
    balances: new Map(),
    /** FA id → { amount, at, from } after dashboard payment settle */
    received: new Map(),
    /** FA id → { amount, fee, at, to } after dashboard payment settle */
    sent: new Map(),
  },
  /** FA id → { status, at } for copilot / liquidity visual overlays */
  nodeVisual: new Map(),
};

export let nodeX = new Float32Array(1025);
export let nodeY = new Float32Array(1025);

export function setNodeArrays(x, y) {
  nodeX = x;
  nodeY = y;
}

export const meshEdges = { ring: [], skip: [], chord: [] };

export function markDirty() {
  state.dirty = true;
}

export function logEvent(text, cls = "") {
  const eventLog = document.getElementById("event-log");
  if (!eventLog) return;
  const li = document.createElement("li");
  li.textContent = `[${new Date().toLocaleTimeString()}] ${text}`;
  if (cls) li.className = cls;
  eventLog.prepend(li);
  while (eventLog.children.length > 40) eventLog.removeChild(eventLog.lastChild);
}

/** Alias for versioned monitor envelopes and embed integrations. */
export function appendLogEvent(text, cls = "") {
  logEvent(text, cls);
}

const NODE_VISUAL_TTL_MS = 120_000;

/** Applies a transient canvas overlay state for a mesh node (copilot drain, hub injection, etc.). */
export function updateNodeVisualState(node, status) {
  if (!node || node < 1 || node > state.networkSize) return;
  state.nodeVisual.set(node, { status, at: Date.now() });
  if (state.nodeVisual.size > 128) {
    const oldest = [...state.nodeVisual.entries()].sort((a, b) => a[1].at - b[1].at)[0];
    if (oldest) state.nodeVisual.delete(oldest[0]);
  }
  markDirty();
}

export function pruneNodeVisualStates(now = Date.now()) {
  for (const [id, meta] of state.nodeVisual) {
    if (now - meta.at > NODE_VISUAL_TTL_MS) state.nodeVisual.delete(id);
  }
}

export function resolveNodeBalances(id) {
  const ledger = state.comm.balances.get(id);
  if (ledger) {
    return { outbound: ledger.outbound, inbound: ledger.inbound };
  }
  const comm = state.comm.nodes.get(id);
  if (comm && (comm.outboundShannons != null || comm.inboundShannons != null)) {
    return {
      outbound: comm.outboundShannons ?? 0,
      inbound: comm.inboundShannons ?? 0,
    };
  }
  return null;
}

export function setNodeLedger(id, outbound, inbound) {
  const now = Date.now();
  state.comm.balances.set(id, { outbound, inbound, at: now });
  const prev = state.comm.nodes.get(id);
  state.comm.nodes.set(id, {
    at: now,
    neighbors: prev?.neighbors ?? [],
    channels: prev?.channels ?? 0,
    outboundShannons: outbound,
    inboundShannons: inbound,
  });
}

export function commEdgeKey(a, b) {
  return a < b ? `${a}-${b}` : `${b}-${a}`;
}

export function pruneComm(now = Date.now()) {
  for (const [id, meta] of state.comm.nodes) {
    if (now - meta.at > COMM_TTL_MS) state.comm.nodes.delete(id);
  }
  for (const [key, meta] of state.comm.edges) {
    if (now - meta.at > COMM_TTL_MS) state.comm.edges.delete(key);
  }
  for (const [id, meta] of state.comm.mfaLinks) {
    if (now - meta.at > COMM_TTL_MS) state.comm.mfaLinks.delete(id);
  }
}

export function touchCommNode(node, neighbors = [], channels = 0, balances = null) {
  if (!node || node < 1 || node > state.networkSize) return;
  const now = Date.now();
  const list = Array.isArray(neighbors) ? neighbors.filter((n) => n >= 1 && n <= state.networkSize) : [];
  const prev = state.comm.nodes.get(node);
  const ledger = state.comm.balances.get(node);

  let outbound = balances?.outbound ?? prev?.outboundShannons ?? ledger?.outbound;
  let inbound = balances?.inbound ?? prev?.inboundShannons ?? ledger?.inbound;
  if (outbound == null && inbound == null && balances) {
    outbound = balances.outbound ?? null;
    inbound = balances.inbound ?? null;
  }

  if (outbound != null || inbound != null) {
    state.comm.balances.set(node, {
      outbound: outbound ?? 0,
      inbound: inbound ?? 0,
      at: now,
    });
  }

  state.comm.nodes.set(node, {
    at: now,
    neighbors: list.length > 0 ? list : (prev?.neighbors ?? []),
    channels: channels > 0 ? channels : (prev?.channels ?? 0),
    outboundShannons: outbound ?? ledger?.outbound ?? null,
    inboundShannons: inbound ?? ledger?.inbound ?? null,
  });
  state.comm.mfaLinks.set(node, { at: now });

  for (const peer of list) {
    state.comm.edges.set(commEdgeKey(node, peer), { at: now, kind: "mesh", a: node, b: peer });
    const peerPrev = state.comm.nodes.get(peer);
    const peerLedger = state.comm.balances.get(peer);
    state.comm.nodes.set(peer, {
      at: now,
      neighbors: peerPrev?.neighbors ?? [],
      channels: peerPrev?.channels ?? 0,
      outboundShannons: peerPrev?.outboundShannons ?? peerLedger?.outbound ?? null,
      inboundShannons: peerPrev?.inboundShannons ?? peerLedger?.inbound ?? null,
    });
  }
  pruneComm(now);
  document.getElementById("metric-live").textContent = String(state.comm.nodes.size);
}

export function touchCommEdge(a, b, kind = "mesh") {
  if (!a || !b || a > state.networkSize || b > state.networkSize) return;
  const now = Date.now();
  state.comm.edges.set(commEdgeKey(a, b), { at: now, kind, a, b });
  for (const node of [a, b]) {
    const prev = state.comm.nodes.get(node);
    const ledger = state.comm.balances.get(node);
    if (prev || ledger) {
      state.comm.nodes.set(node, {
        at: now,
        neighbors: prev?.neighbors ?? [],
        channels: prev?.channels ?? 0,
        outboundShannons: prev?.outboundShannons ?? ledger?.outbound ?? null,
        inboundShannons: prev?.inboundShannons ?? ledger?.inbound ?? null,
      });
    } else {
      touchCommNode(node);
    }
  }
}

export function isCommLive(id, now = Date.now()) {
  const meta = state.comm.nodes.get(id);
  return meta != null && now - meta.at <= COMM_TTL_MS;
}
