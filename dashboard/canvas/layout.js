import { nodeX, nodeY, state } from "../state.js";
import { gridDim } from "../topology.js";
import { requireCanvas } from "../dom.js";

const canvas = requireCanvas("grid");

export function layoutNodes() {
  const pad = 36;
  const w = canvas.width - pad * 2;
  const h = canvas.height - pad * 2;
  const N = state.networkSize;

  const currentGridDim = Math.ceil(Math.sqrt(N));

  for (let id = 1; id <= N; id++) {
    const idx = id - 1;
    const col = idx % currentGridDim;
    const row = Math.floor(idx / currentGridDim);

    const divX = currentGridDim > 1 ? currentGridDim - 1 : 1;
    const divY = currentGridDim > 1 ? currentGridDim - 1 : 1;

    nodeX[id] = pad + (col / divX) * w;
    nodeY[id] = pad + (row / divY) * h;
  }
}

/** @param {number} x @param {number} y @returns {number | null} */
export function nodeAt(x, y) {
  const pad = 36;
  const w = canvas.width - pad * 2;
  const h = canvas.height - pad * 2;
  const dim = gridDim();
  const span = Math.max(1, dim - 1);
  const col = Math.round(((x - pad) / w) * span);
  const row = Math.round(((y - pad) / h) * span);
  if (col < 0 || col >= dim || row < 0 || row >= dim) return null;
  const id = row * dim + col + 1;
  if (id > state.networkSize) return null;
  const d = Math.hypot(nodeX[id] - x, nodeY[id] - y);
  return d < 16 ? id : null;
}

export { canvas };
