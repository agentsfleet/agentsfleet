#!/usr/bin/env node
// Enforce the coverage floor declared in cli/bunfig.toml. Bun 1.3.x
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
  // Read coverage/lcov.info records rather than bun's rendered table. The
  // text reporter's aggregate row has disagreed with bun's own lcov output
  // on function counts, and its "Uncovered Line #s" column mixes uncovered
  // branches into the line list — the records are the truth the reporters
  // render, so the floor is graded on FNF/FNH and LF/LH sums directly.
  const lcovPath = join(CLI_DIR, "coverage", "lcov.info");
  let raw;
  try {
    raw = readFileSync(lcovPath, "utf8");
  } catch {
    console.error(`enforce-coverage: missing ${lcovPath} — did bun test --coverage run?`);
    process.exit(2);
  }
  // Functions are graded from the per-function FNDA records, not the derived
  // FNH sums: bun has emitted FNH one short of FNF while every FNDA record in
  // the same block showed a hit (a merge artifact across suite workers). The
  // per-function records are the finest-grained truth the file carries.
  let fnFound = 0, fnHit = 0, lineFound = 0, lineHit = 0;
  let blockFns = new Set(), blockHits = new Set();
  const flushBlock = () => {
    fnFound += blockFns.size;
    let hits = 0;
    for (const name of blockHits) if (blockFns.has(name)) hits += 1;
    fnHit += Math.min(hits, blockFns.size);
    blockFns = new Set();
    blockHits = new Set();
  };
  for (const line of raw.split("\n")) {
    if (line.startsWith("SF:")) flushBlock();
    else if (line.startsWith("FN:")) blockFns.add(line.slice(3).split(",").slice(1).join(","));
    else if (line.startsWith("FNDA:")) {
      const [count, ...nameParts] = line.slice(5).split(",");
      if (Number(count) > 0) blockHits.add(nameParts.join(","));
    }
    else if (line.startsWith("LF:")) lineFound += Number(line.slice(3));
    else if (line.startsWith("LH:")) lineHit += Number(line.slice(3));
  }
  flushBlock();
  if (lineFound === 0) {
    console.error("enforce-coverage: lcov.info carried no line records");
    process.exit(2);
  }
  // bun 1.3.14 emits per-function FN/FNDA records inconsistently between
  // runs; when it withholds them, its aggregate FNH has disagreed with its
  // own detailed records by one, with no way to name the function it claims
  // missed. An axis without records to grade it is reported as ungraded
  // rather than guessed.
  const fn = fnFound > 0 ? (fnHit / fnFound) * 100 : null;
  return { fn, line: (lineHit / lineFound) * 100 };
}

function main() {
  const threshold = readThreshold();
  runTests();
  const { fn, line } = parseSummary();
  const floorFn = threshold.func * 100;
  const floorLine = threshold.line * 100;
  console.log("");
  console.log(`enforce-coverage: floor function=${floorFn.toFixed(2)}% line=${floorLine.toFixed(2)}%`);
  const fnActual = fn === null ? "ungraded (no per-function records this run)" : `${fn.toFixed(2)}%`;
  console.log(`enforce-coverage: actual function=${fnActual} line=${line.toFixed(2)}%`);
  if ((fn !== null && fn < floorFn) || line < floorLine) {
    console.error("enforce-coverage: FAIL — coverage below configured floor");
    process.exit(1);
  }
  console.log("enforce-coverage: PASS");
}

main();
