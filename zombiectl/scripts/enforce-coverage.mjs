#!/usr/bin/env node
// Enforce the coverage floor declared in zombiectl/bunfig.toml. Bun 1.3.x
// parses `coverageThreshold` but does NOT fail the test run when the
// floor is missed; this script runs `bun test --coverage`, parses the
// "All files" summary, and exits non-zero if either function% or line%
// falls below the configured floor.
//
// Wired into package.json `test` so CI fails on coverage regressions.

import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const SELF = fileURLToPath(import.meta.url);
const ZOMBIECTL_DIR = dirname(dirname(SELF));

function readThreshold() {
  const bunfigPath = join(ZOMBIECTL_DIR, "bunfig.toml");
  const raw = readFileSync(bunfigPath, "utf8");
  const match = raw.match(/coverageThreshold\s*=\s*\{\s*line\s*=\s*([0-9.]+)\s*,\s*function\s*=\s*([0-9.]+)/);
  if (!match) {
    console.error("enforce-coverage: failed to parse coverageThreshold from bunfig.toml");
    process.exit(2);
  }
  return { line: Number(match[1]), func: Number(match[2]) };
}

function runTests() {
  const result = spawnSync("bun", ["test", "--coverage"], {
    cwd: ZOMBIECTL_DIR,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  process.stdout.write(result.stdout ?? "");
  process.stderr.write(result.stderr ?? "");
  if (result.status !== 0) {
    console.error(`enforce-coverage: bun test exited ${result.status}`);
    process.exit(result.status ?? 1);
  }
  return `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
}

function parseSummary(output) {
  // bun's coverage table renders as:
  // All files | <fn%> | <line%> |
  // We match the LAST occurrence to skip any partial frames the runner
  // may emit during incremental output.
  const lines = output.split("\n").filter((l) => /^\s*All files\s*\|/.test(l));
  if (lines.length === 0) {
    console.error("enforce-coverage: could not find 'All files' summary row in test output");
    process.exit(2);
  }
  const last = lines[lines.length - 1];
  const cols = last.split("|").map((s) => s.trim());
  // cols: ['All files', '<fn%>', '<line%>', ...]
  const fn = Number(cols[1]);
  const line = Number(cols[2]);
  if (!Number.isFinite(fn) || !Number.isFinite(line)) {
    console.error(`enforce-coverage: failed to parse summary row: ${last}`);
    process.exit(2);
  }
  return { fn, line };
}

function main() {
  const threshold = readThreshold();
  const output = runTests();
  const { fn, line } = parseSummary(output);
  const floorFn = threshold.func * 100;
  const floorLine = threshold.line * 100;
  console.log("");
  console.log(`enforce-coverage: floor function=${floorFn.toFixed(2)}% line=${floorLine.toFixed(2)}%`);
  console.log(`enforce-coverage: actual function=${fn.toFixed(2)}% line=${line.toFixed(2)}%`);
  if (fn < floorFn || line < floorLine) {
    console.error("enforce-coverage: FAIL — coverage below configured floor");
    process.exit(1);
  }
  console.log("enforce-coverage: PASS");
}

main();
