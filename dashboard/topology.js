import { state, meshEdges, commEdgeKey } from "./state.js";

/** @typedef {'ring' | 'skip' | 'chord'} MeshLinkKind */

/**
 * @typedef {Object} ChannelEdgeDraw
 * @property {number} a
 * @property {number} b
 * @property {number} capacityShannons
 * @property {number} bend
 * @property {number} indexAmongSource
 * @property {number} outboundCount
 */

/** @param {number} [n] @returns {number} */
export function gridDim(n = state.networkSize) {
  return Math.ceil(Math.sqrt(n));
}

/** @param {number} id @param {number} [totalNodes] @returns {number} */
export function oppositePeer(id, totalNodes = state.networkSize) {
  return ((id - 1 + Math.floor(totalNodes / 2)) % totalNodes) + 1;
}

/**
 * @param {number} agentId
 * @param {number} [totalNodes]
 * @returns {Array<{ peer: number, kind: MeshLinkKind }>}
 */
export function meshPeerLinks(agentId, totalNodes = state.networkSize) {
  const i = agentId;
  const ring = i === totalNodes ? 1 : i + 1;
  const skip = i >= totalNodes - 1 ? 1 : i + 2;
  const chord = oppositePeer(i, totalNodes);
  return [
    { peer: ring, kind: "ring" },
    { peer: skip, kind: "skip" },
    { peer: chord, kind: "chord" },
  ];
}

export function buildMeshEdges() {
  meshEdges.ring = [];
  meshEdges.skip = [];
  meshEdges.chord = [];

  const seen = new Set();
  const N = state.networkSize;

  for (let id = 1; id <= N; id++) {
    const ring = id === N ? 1 : id + 1;
    const skip = id >= N - 1 ? 1 : id + 2;
    const chord = oppositePeer(id, N);

    /** @type {Array<[number, MeshLinkKind]>} */
    const links = [[ring, "ring"], [skip, "skip"], [chord, "chord"]];
    for (const [peer, kind] of links) {
      if (peer > N || peer === id) continue;
      const a = Math.min(id, peer);
      const b = Math.max(id, peer);
      const key = `${a}-${b}`;
      if (seen.has(key)) continue;
      seen.add(key);
      meshEdges[kind].push([a, b]);
    }
  }
}

/**
 * Perpendicular bend magnitude for the n-th parallel outbound from a multi-channel node.
 * @param {number} index
 * @param {number} count
 * @param {number} [base]
 */
export function outboundBend(index, count, base = 22) {
  if (count <= 1) return base * 0.4;
  const mid = (count - 1) / 2;
  return (index - mid) * (base * 0.9);
}

/**
 * Quadratic control point for a curved Mesh/Fiber edge.
 * @param {number} x1
 * @param {number} y1
 * @param {number} x2
 * @param {number} y2
 * @param {number} bend
 * @returns {{ cx: number, cy: number }}
 */
export function quadraticControlPoint(x1, y1, x2, y2, bend) {
  const mx = (x1 + x2) / 2;
  const my = (y1 + y2) / 2;
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len = Math.hypot(dx, dy) || 1;
  const nx = -dy / len;
  const ny = dx / len;
  return { cx: mx + nx * bend, cy: my + ny * bend };
}

/**
 * Maps Shannon capacity into stroke weight + color for observability.
 * @param {number} capacityShannons
 * @returns {{ width: number, color: string }}
 */
export function capacityStrokeStyle(capacityShannons) {
  const c = Math.max(0, Number(capacityShannons) || 0);
  const width = 1.15 + Math.min(4.4, Math.log10(c + 10) * 1.15);
  if (c <= 0) {
    return { width: 1.05, color: "rgba(110, 120, 140, 0.4)" };
  }
  if (c < 100_000_000) {
    return { width, color: "rgba(0, 190, 230, 0.72)" };
  }
  if (c < 10_000_000_000) {
    return { width, color: "rgba(70, 210, 130, 0.88)" };
  }
  return { width, color: "rgba(241, 196, 15, 0.92)" };
}

/**
 * Live Fiber/sidecar outbound edges from MFA heartbeats, with multi-curve bends.
 * Prefers `state.channelEdges` capacity; falls back to source ledger outbound.
 * @returns {ChannelEdgeDraw[]}
 */
export function listLiveChannelEdges() {
  /** @type {Map<number, number[]>} */
  const bySource = new Map();

  for (const meta of state.channelEdges.values()) {
    if (!meta?.a || !meta?.b) continue;
    const list = bySource.get(meta.a) ?? [];
    list.push(meta.b);
    bySource.set(meta.a, list);
  }

  if (bySource.size === 0) {
    for (const [id, node] of state.comm.nodes) {
      const neighbors = Array.isArray(node.neighbors) ? node.neighbors : [];
      if (neighbors.length === 0) continue;
      bySource.set(id, [...new Set(neighbors)]);
    }
  }

  /** @type {ChannelEdgeDraw[]} */
  const edges = [];
  for (const [source, peers] of bySource) {
    const sorted = [...peers].filter((p) => p !== source).sort((x, y) => x - y);
    const count = sorted.length;
    for (let i = 0; i < count; i++) {
      const peer = sorted[i];
      const key = commEdgeKey(source, peer);
      const ch = state.channelEdges.get(`${source}->${peer}`)
        ?? state.channelEdges.get(key);
      const ledger = state.comm.balances.get(source);
      const capacity = ch?.capacityShannons
        ?? ledger?.outbound
        ?? state.comm.nodes.get(source)?.outboundShannons
        ?? 0;
      edges.push({
        a: source,
        b: peer,
        capacityShannons: Number(capacity) || 0,
        bend: outboundBend(i, count),
        indexAmongSource: i,
        outboundCount: count,
      });
    }
  }
  return edges;
}
