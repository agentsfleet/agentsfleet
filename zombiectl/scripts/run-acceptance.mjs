#!/usr/bin/env node
/**
 * Runner for the acceptance suite. Iterates `test/acceptance/*.spec.js`
 * via `node --test`. Gates on `ZOMBIE_ACCEPTANCE_TARGET` being set —
 * exits 0 silently when unset so local `bun run test` is unaffected.
 *
 * Empty suite (no spec files) is a green exit by design — `bun run
 * test:acceptance` exits 0 with "no specs" when unset and exits 0
 * with an empty suite when set so local `bun run test` is unaffected.
 */

import fs from "node:fs";
import path from "node:path";
import url from "node:url";
import { spawnSync } from "node:child_process";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const ZOMBIECTL_ROOT = path.resolve(HERE, "..");
const ACCEPTANCE_DIR = path.join(ZOMBIECTL_ROOT, "test", "acceptance");
const TARGET_ENV = "ZOMBIE_ACCEPTANCE_TARGET";

function discoverSpecs() {
  if (!fs.existsSync(ACCEPTANCE_DIR)) return [];
  return fs
    .readdirSync(ACCEPTANCE_DIR)
    .filter((f) => f.endsWith(".spec.js"))
    .map((f) => path.join("test", "acceptance", f))
    .sort();
}

const target = process.env[TARGET_ENV];
if (!target) {
  console.log(`acceptance: ${TARGET_ENV} unset — skipping (set it to https://api-dev.usezombie.com or similar)`);
  process.exit(0);
}

const specs = discoverSpecs();
if (specs.length === 0) {
  console.log(`acceptance: no specs in ${path.relative(ZOMBIECTL_ROOT, ACCEPTANCE_DIR)} — empty suite, exiting 0`);
  process.exit(0);
}

console.log(`acceptance: target=${target}; running ${specs.length} spec file(s)`);
const result = spawnSync(process.execPath, ["--test", ...specs], {
  cwd: ZOMBIECTL_ROOT,
  stdio: "inherit",
  env: process.env,
});
process.exit(result.status ?? 1);
