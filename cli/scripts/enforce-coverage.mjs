#!/usr/bin/env node
// Enforce the coverage floor declared in cli/bunfig.toml. Bun 1.4.x
// parses `coverageThreshold` but does NOT fail the test run when the
// floor is missed; this script runs `bun test --coverage`, parses the
// "All files" summary, and exits non-zero if either function% or line%
// falls below the configured floor.
//
// Wired into package.json `test` so CI fails on coverage regressions.

import { spawnSync } from "node:child_process";
import { readFileSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { gradeLcov } from "./lcov-grade.mjs";

const SELF = fileURLToPath(import.meta.url);
const CLI_DIR = dirname(dirname(SELF));

function readThreshold() {
  const bunfigPath = join(CLI_DIR, "bunfig.toml");
  const raw = readFileSync(bunfigPath, "utf8");
  const match = raw.match(/coverageThreshold\s*=\s*\{\s*line\s*=\s*([0-9.]+)\s*,\s*function\s*=\s*([0-9.]+)/);
  if (!match) {
    console.error("enforce-coverage: failed to parse coverageThreshold from bunfig.toml");
    process.exit(2);
  }
  return { line: Number(match[1]), func: Number(match[2]) };
}

function runTests() {
  // Delete any prior lcov.info first. parseSummary grades from that file, so a
  // run that exits 0 without rewriting it (a dropped reporter, a bun path
  // change) must fail on a missing file, never grade a stale green.
  rmSync(join(CLI_DIR, "coverage", "lcov.info"), { force: true });
  // --timeout 30000: spawn-based help-e2e / PTY tests flake at bun's 5s default
  // under parallel test-lane load; give the built-binary spawns realistic time.
  const result = spawnSync("bun", ["test", "--coverage", "--timeout", "30000"], {
    cwd: CLI_DIR,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  process.stdout.write(result.stdout ?? "");
  process.stderr.write(result.stderr ?? "");
  if (result.status !== 0) {
    console.error(`enforce-coverage: bun test exited ${result.status}`);
    process.exit(result.status ?? 1);
  }
}

function parseSummary() {
  const lcovPath = join(CLI_DIR, "coverage", "lcov.info");
  let raw;
  try {
    raw = readFileSync(lcovPath, "utf8");
  } catch {
    console.error(`enforce-coverage: missing ${lcovPath} — did bun test --coverage run?`);
    process.exit(2);
  }
  try {
    return gradeLcov(raw);
  } catch (err) {
    console.error(`enforce-coverage: ${err instanceof Error ? err.message : String(err)}`);
    process.exit(2);
  }
}

function main() {
  const threshold = readThreshold();
  runTests();
  const { fn, line, uncovered } = parseSummary();
  const floorFn = threshold.func * 100;
  const floorLine = threshold.line * 100;
  console.log("");
  console.log(`enforce-coverage: floor function=${floorFn.toFixed(2)}% line=${floorLine.toFixed(2)}%`);
  console.log(`enforce-coverage: actual function=${fn.toFixed(2)}% line=${line.toFixed(2)}%`);
  if (fn < floorFn || line < floorLine) {
    console.error("enforce-coverage: FAIL — coverage below configured floor");
    if (line < floorLine && uncovered.length > 0) {
      console.error(`enforce-coverage: ${uncovered.length} uncovered line(s):`);
      for (const u of uncovered) console.error(`  ${u}`);
    }
    process.exit(1);
  }
  console.log("enforce-coverage: PASS");
}

main();
