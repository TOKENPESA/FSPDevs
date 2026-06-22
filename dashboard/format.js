import { state } from "./state.js";
import { gridDim } from "./topology.js";

export function formatShannons(shannons) {
  if (shannons == null || Number.isNaN(shannons)) return "—";
  const n = Math.round(Number(shannons));
  return `${n.toLocaleString()} shannons`;
}

export function formatGridDim(n = state.networkSize) {
  const d = gridDim(n);
  return `${d}×${d}`;
}
