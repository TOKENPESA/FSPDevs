/**
 * Shared #grid canvas lifecycle for MFA console.
 *
 * Rules:
 * - Never park under [hidden] / display:none / 0×0 overflow (wipes bitmap + 2d context).
 * - Never use a viewport overlay (leaks onto other routes).
 * - Move canvas into Visualizer host on mount; park before content innerHTML.
 */

const PARK_ID = "mfa-canvas-park";
const CANVAS_ID = "grid";
const DEFAULT_W = 960;
const DEFAULT_H = 520;

function ensurePark() {
  let park = document.getElementById(PARK_ID);
  if (park) return park;
  park = document.createElement("div");
  park.id = PARK_ID;
  park.setAttribute("aria-hidden", "true");
  document.body.appendChild(park);
  return park;
}

/** @returns {HTMLCanvasElement | null} */
function gridCanvas() {
  const el = document.getElementById(CANVAS_ID);
  return el instanceof HTMLCanvasElement ? el : null;
}

/**
 * Reset the canvas backing store so Chromium re-allocates after non-rendered park.
 * @param {HTMLCanvasElement} canvas
 */
function resetBackingStore(canvas) {
  const w = canvas.width > 0 ? canvas.width : DEFAULT_W;
  const h = canvas.height > 0 ? canvas.height : DEFAULT_H;
  canvas.width = w;
  canvas.height = h;
}

async function forceRedraw() {
  const [{ markDirty, state }, { layoutNodes }, { buildMeshEdges }, drawMod] =
    await Promise.all([
      import("../../dashboard/state.js"),
      import("../../dashboard/canvas/layout.js"),
      import("../../dashboard/topology.js"),
      import("../../dashboard/canvas/draw.js"),
    ]);
  if (typeof drawMod.refreshCanvasContext === "function") {
    drawMod.refreshCanvasContext();
  }
  layoutNodes();
  buildMeshEdges();
  state.playing = true;
  markDirty();
  if (typeof drawMod.drawConstellation === "function") {
    drawMod.drawConstellation(performance.now());
  }
}

/** Two frames so layout/CSS (width:100%) settle before first paint. */
function afterLayout() {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve(undefined));
    });
  });
}

/** Move #grid into an off-screen but still-rendered park before route content is replaced. */
export function parkMeshCanvas() {
  const canvas = gridCanvas();
  const park = ensurePark();
  if (!canvas) return;

  canvas.classList.remove("mesh-canvas-live");
  // Keep a real box so the bitmap is not discarded (no visibility:hidden / display:none).
  canvas.style.cssText = [
    "position:fixed",
    "left:-12000px",
    "top:0",
    `width:${DEFAULT_W}px`,
    `height:${DEFAULT_H}px`,
    "pointer-events:none",
    "margin:0",
  ].join(";");

  if (canvas.parentElement !== park) {
    park.appendChild(canvas);
  }
}

/**
 * Place #grid inside the Visualizer host and force a full redraw.
 * @param {Element | null | undefined} host
 */
export async function attachMeshCanvas(host) {
  const canvas = gridCanvas();
  if (!(canvas instanceof HTMLCanvasElement)) return;
  if (!(host instanceof HTMLElement)) {
    parkMeshCanvas();
    return;
  }

  canvas.classList.add("mesh-canvas-live");
  canvas.removeAttribute("style");
  host.prepend(canvas);

  resetBackingStore(canvas);
  await afterLayout();
  await forceRedraw();
}

/** @deprecated use parkMeshCanvas */
export function detachMeshCanvas() {
  parkMeshCanvas();
}
