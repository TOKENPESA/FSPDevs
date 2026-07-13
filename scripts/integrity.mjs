#!/usr/bin/env node
/**
 * Repo integrity gate: JSON parse, Rust compile, ES module import smoke tests.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(fileURLToPath(new URL(".", import.meta.url)), "..");
let failures = 0;

function installBrowserShim() {
  if (globalThis.document) return;

  const makeCtx = () => ({
    clearRect() {},
    fillRect() {},
    strokeRect() {},
    beginPath() {},
    closePath() {},
    moveTo() {},
    lineTo() {},
    arc() {},
    fill() {},
    stroke() {},
    setLineDash() {},
    save() {},
    restore() {},
    translate() {},
    rotate() {},
    scale() {},
    measureText: () => ({ width: 0 }),
    fillText() {},
    strokeText() {},
  });

  const makeEl = () => ({
    addEventListener() {},
    removeEventListener() {},
    querySelector() {
      return null;
    },
    querySelectorAll() {
      return [];
    },
    closest() {
      return makeEl();
    },
    setAttribute() {},
    removeAttribute() {},
    getContext() {
      return makeCtx();
    },
    classList: { add() {}, remove() {}, toggle() {} },
    style: {},
    hidden: false,
    innerHTML: "",
    textContent: "",
    value: "",
    disabled: false,
    options: [],
    width: 960,
    height: 520,
  });

  globalThis.window = globalThis;
  globalThis.window.addEventListener = () => {};
  globalThis.window.removeEventListener = () => {};
  globalThis.window.matchMedia = () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  });
  globalThis.document = {
    getElementById: (id) => {
      const el = makeEl();
      if (id === "grid") {
        el.width = 960;
        el.height = 520;
      }
      return el;
    },
    querySelector: () => makeEl(),
    querySelectorAll: () => [],
    addEventListener() {},
    readyState: "complete",
    createElement: () => makeEl(),
  };
  try {
    Object.defineProperty(globalThis, "navigator", {
      value: { clipboard: { writeText: async () => {} } },
      configurable: true,
    });
  } catch {
    /* Node may expose a read-only navigator — clipboard optional for smoke import */
  }
  globalThis.window.__TAURI__ = undefined;
  globalThis.requestAnimationFrame = (cb) => setTimeout(cb, 0);
  globalThis.cancelAnimationFrame = (id) => clearTimeout(id);
}

function fail(message) {
  failures += 1;
  console.error(`FAIL  ${message}`);
}

function pass(message) {
  console.log(`OK    ${message}`);
}

function walkJsonFiles(dir, out = []) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return out;
  }
  for (const entry of entries) {
    const abs = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (["node_modules", "target", ".git", "pkg"].includes(entry.name)) continue;
      if (entry.name.includes("audit")) continue;
      walkJsonFiles(abs, out);
      continue;
    }
    if (entry.isFile() && entry.name === "package.json") {
      out.push(abs);
    }
  }
  return out;
}

function validateJsonFiles() {
  console.log("\n== JSON ==");
  const files = walkJsonFiles(root).filter((file) => {
    const rel = relative(root, file).replaceAll("\\", "/");
    return (
      rel === "package.json" ||
      rel.startsWith("fiber-agent/") ||
      rel.startsWith("fiber/") ||
      rel.startsWith("Fiber Readiness/")
    );
  });

  for (const file of files) {
    const rel = relative(root, file);
    try {
      JSON.parse(readFileSync(file, "utf8"));
      pass(rel);
    } catch (error) {
      fail(`${rel}: ${error.message}`);
    }
  }
}

function runCargoCheck(label, cwd) {
  const result = spawnSync("cargo", ["check", "-q"], {
    cwd,
    encoding: "utf8",
    shell: process.platform === "win32",
  });
  if (result.status === 0) {
    pass(`cargo check (${label})`);
    return;
  }
  fail(`cargo check (${label})`);
  const stderr = (result.stderr || "").trim();
  const stdout = (result.stdout || "").trim();
  if (stderr) console.error(stderr);
  if (stdout) console.error(stdout);
}

function runCargoAudit(cwd) {
  const result = spawnSync("cargo", ["audit", "-q"], {
    cwd,
    encoding: "utf8",
    shell: process.platform === "win32",
  });
  if (result.status === 0) {
    pass("cargo audit (workspace root)");
    return;
  }
  fail("cargo audit (workspace root) — dependency vulnerabilities detected");
  const stderr = (result.stderr || "").trim();
  const stdout = (result.stdout || "").trim();
  if (stderr) console.error(stderr);
  if (stdout) console.error(stdout);
}

function runCargoTest(label, cwd, extraArgs = []) {
  const result = spawnSync(
    "cargo",
    ["test", "-q", "--", "--test-threads=1", ...extraArgs],
    {
      cwd,
      encoding: "utf8",
      shell: process.platform === "win32",
    },
  );
  if (result.status === 0) {
    pass(`cargo test (${label})`);
    return;
  }
  fail(`cargo test (${label})`);
  const stderr = (result.stderr || "").trim();
  const stdout = (result.stdout || "").trim();
  if (stderr) console.error(stderr);
  if (stdout) console.error(stdout);
}

function validateRust() {
  console.log("\n== Rust ==");
  runCargoCheck("mesh-core", join(root, "mesh-core"));
  runCargoCheck("fsp-fixed-math", join(root, "fsp-fixed-math"));
  runCargoCheck("fiber-agent", join(root, "fiber-agent"));
  runCargoCheck("fiber-agent-desktop", join(root, "fiber-agent/src-tauri"));
  runCargoCheck("master-fiber-agent", join(root, "master-fiber-agent"));
  runCargoAudit(join(root, "."));
  runCargoTest("master-fiber-agent", join(root, "master-fiber-agent"));
}

const IMPORT_SMOKE = [
  "packages/fsp-fixed-math/index.js",
  "dashboard/logger.js",
  "dashboard/money.js",
  "dashboard/main.js",
  "dashboard/events/monitor.js",
  "dashboard/dashboard-module-api.js",
  "dashboard/dashboard-module-ui.js",
  "mfa-console/js/mfa-api.js",
  "mfa-console/js/mfa-runtime.js",
  "mfa-console/js/dashboard-stats.js",
  "mfa-console/js/mfa-module-store-api.js",
  "fiber-agent/src-tauri/frontend/js/app.js",
  "fiber-agent/src-tauri/frontend/js/oob-fallback.js",
  "fiber-agent/src-tauri/frontend/js/dashboard-stats.js",
  "fiber-agent/src-tauri/frontend/js/modules/app-store/app-store-panel.js",
];

async function validateImports() {
  console.log("\n== Import smoke ==");
  installBrowserShim();
  for (const rel of IMPORT_SMOKE) {
    const abs = join(root, rel);
    try {
      await import(pathToFileURL(abs).href);
      pass(rel);
    } catch (error) {
      fail(`${rel}: ${error?.message ?? error}`);
    }
  }
}

function validateMfaCssSync() {
  console.log("\n== MFA CSS sync ==");
  const source = join(root, "fiber-agent/src-tauri/frontend/styles/console.css");
  const target = join(root, "mfa-console/styles/console.css");
  try {
    const a = readFileSync(source, "utf8");
    const b = readFileSync(target, "utf8");
    if (a === b) {
      pass("mfa-console/styles/console.css matches sidecar console.css");
    } else {
      fail("mfa-console/styles/console.css is out of sync — run npm run sync:mfa-css");
    }
  } catch (error) {
    fail(`MFA CSS sync check: ${error.message}`);
  }
}

function validateConsoleBuild() {
  console.log("\n== FSP console build ==");
  const result = spawnSync("npm", ["run", "build", "--prefix", "fsp-console"], {
    cwd: root,
    encoding: "utf8",
    shell: process.platform === "win32",
  });
  if (result.status === 0) {
    pass("fsp-console vite build");
    return;
  }
  fail("fsp-console vite build");
  const stderr = (result.stderr || "").trim();
  const stdout = (result.stdout || "").trim();
  if (stderr) console.error(stderr);
  if (stdout) console.error(stdout);
}

console.log("FSPDevs integrity check");
validateJsonFiles();
validateRust();
await validateImports();
validateMfaCssSync();
validateConsoleBuild();

console.log(`\nDone: ${failures} failure(s)`);
process.exit(failures > 0 ? 1 : 0);
