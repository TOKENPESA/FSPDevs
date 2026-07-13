import { createLogger } from "./dashboard/logger.js";
import { initSidecarConsole } from "./sidecar-console.js";

const log = createLogger("sidecar-console");

initSidecarConsole().catch((err) => {
  log.error("init failed", err);
  const stream = document.getElementById("console-stream");
  if (stream) {
    const line = document.createElement("div");
    line.className = "log-line danger";
    line.textContent = `[BOOT ERROR] ${err}`;
    stream.appendChild(line);
  }
});
