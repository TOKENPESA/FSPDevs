import { state, meshEdges } from "./state.js";

/** @typedef {'ring' | 'skip' | 'chord'} MeshLinkKind */

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
