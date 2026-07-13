import { state } from "./state.js";
import { gridDim } from "./topology.js";

export function formatGridDim(n = state.networkSize) {
  const d = gridDim(n);
  return `${d}×${d}`;
}
