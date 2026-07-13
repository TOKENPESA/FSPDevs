#!/usr/bin/env node
/**
 * Sync dashboard + package deps into the Tauri frontend bundle (frontendDist).
 * Required for ES module imports to resolve under Tauri's asset scope.
 */
import { copyFileSync, cpSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const frontend = join(root, "fiber-agent", "src-tauri", "frontend");
const dashSrc = join(root, "dashboard");
const dashDest = join(frontend, "dashboard");
const packagesDest = join(frontend, "packages");

const DASHBOARD_FILES = [
  "logger.js",
  "money.js",
  "config.js",
  "fetch-timeout.js",
  "dashboard-module-api.js",
  "dashboard-module-ui.js",
  "dom.js",
];

const PACKAGE_DIRS = ["fsp-fixed-math", "fsp-ui-types"];

mkdirSync(dashDest, { recursive: true });
mkdirSync(packagesDest, { recursive: true });

for (const file of DASHBOARD_FILES) {
  const src = join(dashSrc, file);
  const dest = join(dashDest, file);
  copyFileSync(src, dest);
  console.log(`  dashboard/${file}`);
}

const domForFrontend = readFileSync(join(dashSrc, "dom.js"), "utf8").replace(
  "../packages/fsp-ui-types/dom.js",
  "./packages/fsp-ui-types/dom.js",
);
writeFileSync(join(frontend, "dom.js"), domForFrontend);
console.log("  dom.js (frontend root)");

const sidecarConsoleDest = join(frontend, "sidecar-console.js");
let sidecarConsole = readFileSync(join(dashSrc, "sidecar-console.js"), "utf8");
sidecarConsole = sidecarConsole
  .replace('from "./dom.js"', 'from "./dom.js"')
  .replace('from "../packages/fsp-ui-types/errors.js"', 'from "./packages/fsp-ui-types/errors.js"');
writeFileSync(sidecarConsoleDest, sidecarConsole);
console.log("  sidecar-console.js");

for (const pkg of PACKAGE_DIRS) {
  const src = join(root, "packages", pkg);
  const dest = join(packagesDest, pkg);
  cpSync(src, dest, { recursive: true });
  console.log(`  packages/${pkg}/`);
}

console.log(`\nSynced sidecar UI into ${frontend}`);
