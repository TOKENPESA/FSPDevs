const RING_SIZE = 1024;
const GRID_DIM = 32;

const canvas = document.getElementById("grid");
const ctx = canvas.getContext("2d");
const eventLog = document.getElementById("event-log");
const connStatus = document.getElementById("conn-status");
const connDot = document.getElementById("conn-dot");
const metricTick = document.getElementById("metric-tick");
const metricDead = document.getElementById("metric-dead");
const metricHeals = document.getElementById("metric-heals");
const metricHover = document.getElementById("metric-hover");
const speedInput = document.getElementById("speed");
const speedLabel = document.getElementById("speed-label");
const mfaWsInput = document.getElementById("mfa-ws");
const faTooltip = document.getElementById("fa-tooltip");
const canvasWrap = canvas.closest(".canvas-wrap");
const routeSourceInput = document.getElementById("route-source");
const routeDestInput = document.getElementById("route-dest");
const routeAmountInput = document.getElementById("route-amount");
const metricRoute = document.getElementById("metric-route");

const MFA_ROUTE_URL = "http://127.0.0.1:1025/route";

const state = {
  dead: new Set(),
  healed: new Set(),
  healLinks: [],
  activeRoute: [],
  tick: 0,
  healCount: 0,
  playing: true,
  speed: 1,
  ws: null,
  hoveredNode: null,
  dirty: true,
  lastFrame: 0,
};

// Typed arrays — lower memory + cache-friendly vs 1024 objects
const nodeX = new Float32Array(RING_SIZE + 1);
const nodeY = new Float32Array(RING_SIZE + 1);

const meshEdges = { ring: [], skip: [], chord: [] };

const MFA_HUB = { x: 52, y: 52 };

const PATH_STYLES = {
  ring: { color: "#50ff9a", width: 2.8, dash: [7, 5], speed: 0.004, label: "Ring +1" },
  skip: { color: "#5eb5ff", width: 2.8, dash: [9, 6], speed: 0.005, label: "Skip +2" },
  chord: { color: "#c678ff", width: 2.8, dash: [5, 7], speed: 0.003, label: "Opposite" },
  mfa: { color: "#ffb347", width: 2.2, dash: [11, 9], speed: 0.006, label: "MFA uplink" },
};

function meshPeerLinks(agentId, totalNodes = RING_SIZE) {
  const i = agentId;
  const ring = i === totalNodes ? 1 : i + 1;
  const skip = i >= totalNodes - 1 ? 1 : i + 2;
  const chord = (i + totalNodes / 2) % totalNodes + 1;
  return [
    { peer: ring, kind: "ring" },
    { peer: skip, kind: "skip" },
    { peer: chord, kind: "chord" },
  ];
}

function buildMeshEdges() {
  const seen = new Set();
  for (let id = 1; id <= RING_SIZE; id++) {
    const ring = id === RING_SIZE ? 1 : id + 1;
    const skip = id >= RING_SIZE - 1 ? 1 : id + 2;
    const chord = (id + RING_SIZE / 2) % RING_SIZE + 1;
    for (const [peer, kind] of [[ring, "ring"], [skip, "skip"], [chord, "chord"]]) {
      const a = Math.min(id, peer);
      const b = Math.max(id, peer);
      const key = `${a}-${b}`;
      if (seen.has(key)) continue;
      seen.add(key);
      meshEdges[kind].push([a, b]);
    }
  }
}

function meshPeersFor(agentId, totalNodes = RING_SIZE) {
  return meshPeerLinks(agentId, totalNodes).map((l) => l.peer);
}

function drawFlashLine(x1, y1, x2, y2, style, now, dead = false) {
  const pulse = dead ? 0.25 : 0.65 + 0.35 * Math.sin(now * 0.009 * state.speed);
  const offset = (now * style.speed * state.speed * 80) % 40;

  ctx.save();
  ctx.lineWidth = style.width;
  ctx.strokeStyle = dead ? "#666" : style.color;
  ctx.globalAlpha = pulse;
  ctx.setLineDash(style.dash);
  ctx.lineDashOffset = -offset;
  ctx.shadowColor = dead ? "transparent" : style.color;
  ctx.shadowBlur = dead ? 0 : 8;
  ctx.beginPath();
  ctx.moveTo(x1, y1);
  ctx.lineTo(x2, y2);
  ctx.stroke();

  if (!dead) {
    const t = (now * 0.0018 * state.speed) % 1;
    ctx.setLineDash([]);
    ctx.shadowBlur = 12;
    ctx.globalAlpha = 1;
    ctx.fillStyle = style.color;
    ctx.beginPath();
    ctx.arc(x1 + (x2 - x1) * t, y1 + (y2 - y1) * t, 3.2, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.restore();
}

function drawHoveredPaths(now) {
  const id = state.hoveredNode;
  if (!id) return;

  for (const { peer, kind } of meshPeerLinks(id)) {
    const dead = state.dead.has(id) || state.dead.has(peer);
    drawFlashLine(
      nodeX[id], nodeY[id],
      nodeX[peer], nodeY[peer],
      PATH_STYLES[kind], now, dead,
    );
  }

  // Telemetry / WS uplink flash to MFA hub (127.0.0.1:1025)
  drawFlashLine(
    nodeX[id], nodeY[id],
    MFA_HUB.x, MFA_HUB.y,
    PATH_STYLES.mfa, now, state.dead.has(id),
  );

  ctx.save();
  ctx.globalAlpha = 0.9;
  ctx.fillStyle = "#ffb347";
  ctx.shadowColor = "#ffb347";
  ctx.shadowBlur = 10;
  ctx.beginPath();
  ctx.arc(MFA_HUB.x, MFA_HUB.y, 5, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = "#fff";
  ctx.font = "9px ui-monospace, monospace";
  ctx.shadowBlur = 0;
  ctx.fillText("MFA", MFA_HUB.x + 8, MFA_HUB.y + 3);
  ctx.restore();
}

function linkActive(a, b) {
  return !state.dead.has(a) && !state.dead.has(b);
}

function drawMeshLayer(edges, style, now, highlightSet) {
  ctx.lineWidth = style.width;
  ctx.strokeStyle = style.color;
  ctx.globalAlpha = style.alpha;
  ctx.beginPath();
  for (const [a, b] of edges) {
    if (!linkActive(a, b)) continue;
    if (highlightSet && (highlightSet.has(a) || highlightSet.has(b))) continue;
    ctx.moveTo(nodeX[a], nodeY[a]);
    ctx.lineTo(nodeX[b], nodeY[b]);
  }
  ctx.stroke();

  if (highlightSet) {
    ctx.strokeStyle = style.hover;
    ctx.globalAlpha = 0.95;
    ctx.beginPath();
    for (const [a, b] of edges) {
      if (!linkActive(a, b)) continue;
      if (!highlightSet.has(a) && !highlightSet.has(b)) continue;
      ctx.moveTo(nodeX[a], nodeY[a]);
      ctx.lineTo(nodeX[b], nodeY[b]);
    }
    ctx.stroke();
  }
  ctx.globalAlpha = 1;
}

function logEvent(text, cls = "") {
  const li = document.createElement("li");
  li.textContent = `[${new Date().toLocaleTimeString()}] ${text}`;
  if (cls) li.className = cls;
  eventLog.prepend(li);
  while (eventLog.children.length > 40) eventLog.removeChild(eventLog.lastChild);
}

function layoutNodes() {
  const pad = 36;
  const w = canvas.width - pad * 2;
  const h = canvas.height - pad * 2;
  for (let id = 1; id <= RING_SIZE; id++) {
    const idx = id - 1;
    const col = idx % GRID_DIM;
    const row = Math.floor(idx / GRID_DIM);
    nodeX[id] = pad + (col / (GRID_DIM - 1)) * w;
    nodeY[id] = pad + (row / (GRID_DIM - 1)) * h;
  }
}

function drawGlowDot(x, y, radius, color, alpha, glow = 8) {
  ctx.globalAlpha = alpha * 0.3;
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.arc(x, y, radius + glow, 0, Math.PI * 2);
  ctx.fill();
  ctx.globalAlpha = alpha;
  ctx.beginPath();
  ctx.arc(x, y, radius, 0, Math.PI * 2);
  ctx.fill();
}

function drawConstellation(now) {
  ctx.fillStyle = "#060a12";
  ctx.fillRect(0, 0, canvas.width, canvas.height);

  if (state.playing) {
    for (let i = 0; i < 60; i++) {
      const sx = (Math.sin(i * 127.1) * 0.5 + 0.5) * canvas.width;
      const sy = (Math.cos(i * 311.7) * 0.5 + 0.5) * canvas.height;
      ctx.globalAlpha = (0.3 + 0.7 * Math.abs(Math.sin(now * 0.001 + i))) * 0.35;
      ctx.fillStyle = "#8ba3cc";
      ctx.fillRect(sx, sy, 1.2, 1.2);
    }
  }

  const hoverMesh = new Set();
  if (state.hoveredNode) {
    hoverMesh.add(state.hoveredNode);
    for (const p of meshPeersFor(state.hoveredNode)) hoverMesh.add(p);
  }

  drawMeshLayer(meshEdges.chord, {
    color: "rgba(155, 89, 182, 0.22)",
    hover: "rgba(192, 120, 255, 0.75)",
    width: 0.8,
    alpha: 0.22,
  }, now, hoverMesh);

  drawMeshLayer(meshEdges.skip, {
    color: "rgba(61, 139, 253, 0.28)",
    hover: "rgba(100, 180, 255, 0.85)",
    width: 0.9,
    alpha: 0.28,
  }, now, hoverMesh);

  drawMeshLayer(meshEdges.ring, {
    color: "rgba(46, 204, 113, 0.35)",
    hover: "rgba(80, 255, 160, 0.9)",
    width: 1,
    alpha: 0.35,
  }, now, hoverMesh);

  drawHoveredPaths(now);
  drawActiveRoute(now);

  for (const link of state.healLinks) {
    ctx.strokeStyle = "rgba(0, 212, 255, 0.85)";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(nodeX[link.from], nodeY[link.from]);
    ctx.lineTo(nodeX[link.to], nodeY[link.to]);
    ctx.stroke();
  }

  const pulse = state.playing ? 0.55 + 0.45 * Math.sin(now * 0.004 * state.speed) : 0.75;
  for (let id = 1; id <= RING_SIZE; id++) {
    const isDead = state.dead.has(id);
    const isHovered = state.hoveredNode === id;
    let color = "#2ecc71";
    let radius = 1.6;
    let alpha = pulse;
    if (isDead) {
      color = "#e74c3c";
      radius = 2.4;
      alpha = 0.85 + 0.15 * Math.sin(now * 0.006);
    } else if (state.healed.has(id)) {
      color = "#00d4ff";
      radius = 2.2;
    }
    if (isHovered) {
      radius += 1.2;
      alpha = 1;
    }
    drawGlowDot(nodeX[id], nodeY[id], radius, color, alpha, isHovered ? 12 : 6);
  }

  metricTick.textContent = String(state.tick);
  metricDead.textContent = String(state.dead.size);
  metricHeals.textContent = String(state.healCount);
  state.dirty = false;
}

function nodeAt(x, y) {
  const pad = 36;
  const w = canvas.width - pad * 2;
  const h = canvas.height - pad * 2;
  const col = Math.round(((x - pad) / w) * (GRID_DIM - 1));
  const row = Math.round(((y - pad) / h) * (GRID_DIM - 1));
  if (col < 0 || col >= GRID_DIM || row < 0 || row >= GRID_DIM) return null;
  const id = row * GRID_DIM + col + 1;
  const d = Math.hypot(nodeX[id] - x, nodeY[id] - y);
  return d < 16 ? id : null;
}

function nodeStatus(id) {
  if (state.dead.has(id)) {
    return { label: "OFFLINE", cls: "offline" };
  }
  if (state.healed.has(id)) {
    return { label: "HEALED / REROUTED", cls: "healed" };
  }
  return { label: "ACTIVE", cls: "active" };
}

function updateTooltip(id, clientX, clientY) {
  if (!id) {
    faTooltip.classList.remove("visible");
    faTooltip.setAttribute("aria-hidden", "true");
    return;
  }

  const status = nodeStatus(id);
  const links = meshPeerLinks(id);
  const livePeers = links.filter((l) => !state.dead.has(l.peer));
  const idx = id - 1;
  const col = idx % GRID_DIM;
  const row = Math.floor(idx / GRID_DIM);

  const linkRows = links.map(({ peer, kind }) => {
    const st = PATH_STYLES[kind];
    const live = !state.dead.has(peer);
    return `<div class="fa-row"><span style="color:${st.color}">●</span> ${st.label}: <span>FA-${peer}${live ? "" : " (down)"}</span></div>`;
  }).join("");

  faTooltip.innerHTML = `
    <div class="fa-id">FA-${id}</div>
    <div class="fa-status ${status.cls}">${status.label}</div>
    <div class="fa-row">Grid: <span>[${col}, ${row}]</span></div>
    <div class="fa-row">Mesh peers: <span>${livePeers.length}/3 live</span></div>
    ${linkRows}
    <div class="fa-row"><span style="color:${PATH_STYLES.mfa.color}">●</span> ${PATH_STYLES.mfa.label}: <span>127.0.0.1:1025</span></div>
  `;

  const wrapRect = canvasWrap.getBoundingClientRect();
  let left = clientX - wrapRect.left + 14;
  let top = clientY - wrapRect.top + 14;
  faTooltip.classList.add("visible");
  faTooltip.setAttribute("aria-hidden", "false");

  // Measure after visible to clamp inside canvas
  const tipRect = faTooltip.getBoundingClientRect();
  if (left + tipRect.width > wrapRect.width - 8) {
    left = clientX - wrapRect.left - tipRect.width - 14;
  }
  if (top + tipRect.height > wrapRect.height - 8) {
    top = clientY - wrapRect.top - tipRect.height - 14;
  }
  faTooltip.style.left = `${Math.max(8, left)}px`;
  faTooltip.style.top = `${Math.max(8, top)}px`;
}

function hideTooltip() {
  faTooltip.classList.remove("visible");
  faTooltip.setAttribute("aria-hidden", "true");
}

function readRouteNode(input) {
  const n = Number.parseInt(input.value, 10);
  if (!Number.isInteger(n) || n < 1 || n > RING_SIZE) {
    throw new Error(`Node must be an integer between 1 and ${RING_SIZE}`);
  }
  return n;
}

function drawActiveRoute(now) {
  const path = state.activeRoute;
  if (path.length < 2) return;

  const pulse = 0.6 + 0.4 * Math.sin(now * 0.008 * state.speed);
  ctx.save();
  ctx.strokeStyle = "#00ffff";
  ctx.lineWidth = 4 + 1.5 * Math.sin(now * 0.006 * state.speed);
  ctx.globalAlpha = pulse;
  ctx.shadowColor = "#00d4ff";
  ctx.shadowBlur = 18;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.beginPath();
  ctx.moveTo(nodeX[path[0]], nodeY[path[0]]);
  for (let i = 1; i < path.length; i++) {
    ctx.lineTo(nodeX[path[i]], nodeY[path[i]]);
  }
  ctx.stroke();

  const totalSegs = path.length - 1;
  const travel = (now * 0.0012 * state.speed) % totalSegs;
  const segIdx = Math.floor(travel);
  const segT = travel - segIdx;
  const from = path[segIdx];
  const to = path[Math.min(segIdx + 1, path.length - 1)];
  const dotX = nodeX[from] + (nodeX[to] - nodeX[from]) * segT;
  const dotY = nodeY[from] + (nodeY[to] - nodeY[from]) * segT;

  ctx.shadowBlur = 22;
  ctx.globalAlpha = 1;
  ctx.fillStyle = "#e8ffff";
  ctx.beginPath();
  ctx.arc(dotX, dotY, 5, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
}

async function routeTransaction() {
  let source;
  let destination;
  let amount;

  try {
    source = readRouteNode(routeSourceInput);
    destination = readRouteNode(routeDestInput);
    amount = Number.parseInt(routeAmountInput.value, 10);
    if (!Number.isFinite(amount) || amount < 1) {
      throw new Error("Amount must be a positive integer");
    }
  } catch (err) {
    logEvent(err.message, "warn");
    return;
  }

  logEvent(`Routing FA-${source} → FA-${destination} (${amount} shannons)…`);

  try {
    const res = await fetch(MFA_ROUTE_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        source,
        destination,
        amount_shannons: amount,
      }),
    });

    const data = await res.json();

    if (data.status === "ROUTE_FOUND" && Array.isArray(data.path) && data.path.length >= 2) {
      state.activeRoute = data.path;
      const hops = data.path.length - 1;
      const pathLabel = data.path.map((id) => `FA-${id}`).join(" → ");
      metricRoute.textContent = `${hops} hop${hops === 1 ? "" : "s"}`;
      logEvent(`ROUTE_FOUND (${data.execution_latency_ms}ms): ${pathLabel}`, "heal");
      markDirty();
    } else {
      state.activeRoute = [];
      metricRoute.textContent = "—";
      logEvent(`Route failed: ${data.status || res.status}`, "warn");
      markDirty();
    }
  } catch (err) {
    state.activeRoute = [];
    metricRoute.textContent = "—";
    logEvent(`Route request failed — is MFA running on :1025? (${err.message})`, "warn");
    markDirty();
  }
}

function markDirty() {
  state.dirty = true;
}

function handleMonitorMessage(raw) {
  let payload;
  try {
    payload = JSON.parse(raw);
  } catch {
    logEvent(`Ignored: ${raw}`);
    return;
  }
  state.tick += 1;
  markDirty();
  if (payload.event === "MESH_HEAL") {
    state.healCount += 1;
    state.dead.add(payload.removed);
    state.healed.add(payload.added);
    state.healLinks.push({ from: payload.node, to: payload.added });
    if (state.healLinks.length > 8) state.healLinks.shift();
    logEvent(
      `MESH_HEAL: FA-${payload.node} swapped FA-${payload.removed} → FA-${payload.added}`,
      "heal",
    );
    if (state.hoveredNode) markDirty();
  } else {
    logEvent(JSON.stringify(payload));
  }
}

function connectMonitor() {
  if (state.ws) {
    state.ws.close();
    state.ws = null;
  }
  const url = mfaWsInput.value.trim();
  if (!url.startsWith("ws://127.0.0.1") && !url.startsWith("ws://localhost")) {
    logEvent("Monitor URL must be ws://127.0.0.1 or ws://localhost", "warn");
    return;
  }
  const ws = new WebSocket(url);
  state.ws = ws;
  ws.onopen = () => {
    connStatus.textContent = "Connected";
    connDot.classList.add("connected");
    logEvent(`Monitor connected: ${url}`, "heal");
  };
  ws.onclose = () => {
    connStatus.textContent = "Disconnected";
    connDot.classList.remove("connected");
    logEvent("Monitor disconnected", "warn");
  };
  ws.onerror = () => logEvent("WebSocket error — is MFA running on :1025?", "warn");
  ws.onmessage = (ev) => handleMonitorMessage(ev.data);
}

let mouseRaf = 0;
canvas.addEventListener("mousemove", (ev) => {
  if (mouseRaf) return;
  mouseRaf = requestAnimationFrame(() => {
    mouseRaf = 0;
    const rect = canvas.getBoundingClientRect();
    const x = (ev.clientX - rect.left) * (canvas.width / rect.width);
    const y = (ev.clientY - rect.top) * (canvas.height / rect.height);
    const id = nodeAt(x, y);
    if (id !== state.hoveredNode) {
      state.hoveredNode = id;
      if (id) {
        const st = nodeStatus(id);
        metricHover.textContent = `FA-${id} · ${st.label}`;
      } else {
        metricHover.textContent = "—";
      }
      markDirty();
    }
    updateTooltip(id, ev.clientX, ev.clientY);
  });
});

canvas.addEventListener("mouseleave", () => {
  state.hoveredNode = null;
  metricHover.textContent = "—";
  hideTooltip();
  markDirty();
});

canvas.addEventListener("click", (ev) => {
  const rect = canvas.getBoundingClientRect();
  const x = (ev.clientX - rect.left) * (canvas.width / rect.width);
  const y = (ev.clientY - rect.top) * (canvas.height / rect.height);
  const id = nodeAt(x, y);
  if (!id) return;
  if (state.dead.has(id)) {
    state.dead.delete(id);
    logEvent(`FA-${id} restored (local)`, "heal");
  } else {
    state.dead.add(id);
    logEvent(`FA-${id} marked offline (local)`, "dead");
  }
  markDirty();
  if (state.hoveredNode === id) {
    updateTooltip(id, ev.clientX, ev.clientY);
    const st = nodeStatus(id);
    metricHover.textContent = `FA-${id} · ${st.label}`;
  }
});

document.getElementById("btn-connect").addEventListener("click", connectMonitor);
document.getElementById("btn-route").addEventListener("click", () => {
  routeTransaction();
});
document.getElementById("btn-play").addEventListener("click", () => {
  state.playing = true;
  markDirty();
  logEvent("Animation playing");
});
document.getElementById("btn-pause").addEventListener("click", () => {
  state.playing = false;
  markDirty();
  logEvent("Animation paused");
});
speedInput.addEventListener("input", () => {
  state.speed = Number(speedInput.value);
  speedLabel.textContent = `${state.speed}×`;
  markDirty();
});

layoutNodes();
buildMeshEdges();

function frame(now) {
  if (state.dirty || state.playing || state.hoveredNode || state.activeRoute.length > 0) {
    state.lastFrame = now;
    drawConstellation(now);
  }
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
connectMonitor();
