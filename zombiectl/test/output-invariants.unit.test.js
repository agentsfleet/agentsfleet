// Repo-level invariants for the design-system rollout. These are
// regression guards: they grep the source tree for patterns that would
// dilute the pulse-cyan currency or leak inline ANSI escapes outside
// the centralized palette module. Failing here means a future edit
// reintroduced the exact thing this spec retired.

import { describe, test, expect } from "bun:test";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, relative } from "node:path";

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const PKG_ROOT = dirname(TEST_DIR);
const SRC_DIR = join(PKG_ROOT, "src");
const PALETTE_PATH = join(SRC_DIR, "output", "palette.js");

function* walk(dir) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const s = statSync(full);
    if (s.isDirectory()) yield* walk(full);
    else if (s.isFile() && full.endsWith(".js")) yield full;
  }
}

function readSource(path) {
  return { path: relative(PKG_ROOT, path), text: readFileSync(path, "utf8") };
}

describe("pulse-cyan currency rule", () => {
  test("palette.pulse / palette.pulseBold called from at most 3 source files", () => {
    const callsites = [];
    for (const path of walk(SRC_DIR)) {
      const { text, path: rel } = readSource(path);
      if (/\bpalette\.pulse(Bold)?\b/.test(text)) callsites.push(rel);
    }
    // The expected callsites: glyph.js (live glyph) and format.js
    // (helpHeading + helper internals). Banner.js composes through
    // glyph.live() and never calls palette.pulse directly.
    expect(callsites.length).toBeLessThanOrEqual(3);
    expect(callsites).toContain("src/output/glyph.js");
    expect(callsites).toContain("src/output/format.js");
  });
});

describe("no inline ANSI escapes outside palette.js", () => {
  test("\\u001b[ literal appears only in src/output/palette.js", () => {
    const violators = [];
    for (const path of walk(SRC_DIR)) {
      if (path === PALETTE_PATH) continue;
      const { text, path: rel } = readSource(path);
      // The exact escape literal that callers used to inline before this
      // package landed. Detect both  and the raw 0x1b char.
      if (text.includes("\\u001b[") || text.includes("[")) {
        violators.push(rel);
      }
    }
    expect(violators).toEqual([]);
  });

  test("256-color codes (38;5;NNN) appear only in palette.js", () => {
    const violators = [];
    for (const path of walk(SRC_DIR)) {
      if (path === PALETTE_PATH) continue;
      const { text, path: rel } = readSource(path);
      if (/38;5;\d+/.test(text)) violators.push(rel);
    }
    expect(violators).toEqual([]);
  });
});

describe("decorative-ASCII teardown", () => {
  test("no zombie face emoji in src/", () => {
    const offenders = [];
    for (const path of walk(SRC_DIR)) {
      const { text, path: rel } = readSource(path);
      if (text.includes("\u{1F9DF}")) offenders.push(rel); // 🧟
    }
    expect(offenders).toEqual([]);
  });

  test("no party emoji in src/", () => {
    const offenders = [];
    for (const path of walk(SRC_DIR)) {
      const { text, path: rel } = readSource(path);
      if (text.includes("\u{1F389}")) offenders.push(rel); // 🎉
    }
    expect(offenders).toEqual([]);
  });

  test("no box-drawing borders in src/program/banner.js", () => {
    const banner = readSource(join(SRC_DIR, "program", "banner.js"));
    for (const ch of ["╭", "╮", "╯", "╰", "│"]) {
      expect(banner.text).not.toContain(ch);
    }
  });
});

describe("file length cap (RULE FLL — ≤350 lines)", () => {
  test("every src file under 350 lines", () => {
    const overages = [];
    for (const path of walk(SRC_DIR)) {
      const { text, path: rel } = readSource(path);
      const lines = text.split("\n").length;
      if (lines > 350) overages.push(`${rel}: ${lines}`);
    }
    expect(overages).toEqual([]);
  });
});
