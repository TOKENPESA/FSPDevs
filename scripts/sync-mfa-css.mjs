#!/usr/bin/env node
/**
 * Sync MFA Operations Console base styles from the Sidecar redesign.
 * MFA-specific rules stay in mfa-console/styles/mfa-extensions.css.
 */
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const source = join(root, "fiber-agent/src-tauri/frontend/styles/console.css");
const target = join(root, "mfa-console/styles/console.css");

mkdirSync(dirname(target), { recursive: true });
copyFileSync(source, target);
console.log(`Synced MFA console CSS\n  from ${source}\n    to ${target}`);
