/**
 * Unit tests for src/program/banner.js — printVersion and printPreReleaseWarning.
 *
 * Coverage tiers addressed:
 *   T1  Happy path
 *   T2  Edge cases
 *   T3  Negative / error paths (void functions — suppression paths covered here)
 *   T4  Output fidelity (actual rendered text and ANSI correctness)
 *   T5  Concurrency — N/A (pure write functions, no shared state)
 *   T6  Integration via runCli ttyOnly flag
 *   T7  Regression guards (email, version constants, design-system invariants pinned)
 *   T8  Security — N/A (no user input, no secret handling)
 *   T9  DRY — shared helpers from helpers.js
 *   T10 Constants / magic values flagged
 *   T11 Performance — N/A (simple stream writes, no allocation concern)
 *   T12 CLI contract (--version output, --json suppression, VERSION matches package.json)
 */

import { describe, test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { makeBufferStream } from "./helpers.js";
import { printVersion, printPreReleaseWarning } from "../src/program/banner.js";
import { runCli, VERSION } from "../src/cli.js";

// ── T9: helpers — no magic, no copy-paste ─────────────────────────────────────

/** Simulate a TTY stream by setting isTTY = true on the underlying writable. */
function makeTtyBufferStream() {
  const b = makeBufferStream();
  b.stream.isTTY = true;
  return b;
}

/** Strip all ANSI escape sequences from a string for plain-text assertions. */
function stripAnsi(str) {
  return str.replace(/\x1b\[[0-9;]*m/g, "");
}

// ── T10: constants guard — pin strings that appear in both code paths ─────────
const CONTACT_EMAIL = "nkishore@megam.io";
const PRE_RELEASE_TAG = "[PRE-RELEASE]";

// Decorative-ASCII teardown — these MUST NOT appear in the version banner
// (per docs/DESIGN_SYSTEM.md "no decorative ASCII art"). Regression guards.
const FORBIDDEN_BANNER_CHARS = [
  "\u{1F9DF}", // 🧟 zombie face
  "╭",    // ╭ box top-left
  "╮",    // ╮ box top-right
  "╰",    // ╰ box bottom-left
  "╯",    // ╯ box bottom-right
  "│",    // │ box vertical
];

// ── printPreReleaseWarning ─────────────────────────────────────────────────────

describe("printPreReleaseWarning — T1: happy path", () => {
  test("default opts writes non-empty output (color mode)", () => {
    const out = makeTtyBufferStream();
    printPreReleaseWarning(out.stream, {});
    expect(out.read().length).toBeGreaterThan(0);
  });

  test("noColor=true writes non-empty plain output", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { noColor: true });
    expect(out.read().length).toBeGreaterThan(0);
  });

  test("jsonMode=true suppresses all output", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { jsonMode: true });
    expect(out.read()).toBe("");
  });

  test("ttyOnly=true suppresses all output", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { ttyOnly: true });
    expect(out.read()).toBe("");
  });
});

describe("printPreReleaseWarning — T3: suppression paths", () => {
  test("jsonMode suppresses regardless of noColor", () => {
    for (const noColor of [true, false]) {
      const out = makeBufferStream();
      printPreReleaseWarning(out.stream, { jsonMode: true, noColor });
      expect(out.read()).toBe("");
    }
  });

  test("ttyOnly suppresses regardless of noColor", () => {
    for (const noColor of [true, false]) {
      const out = makeBufferStream();
      printPreReleaseWarning(out.stream, { ttyOnly: true, noColor });
      expect(out.read()).toBe("");
    }
  });
});

describe("printPreReleaseWarning — T4: fidelity (color mode)", () => {
  test("color output contains warning glyph and email", () => {
    const out = makeTtyBufferStream();
    printPreReleaseWarning(out.stream, {});
    const txt = out.read();
    expect(txt).toContain("⚠");
    expect(stripAnsi(txt)).toContain(CONTACT_EMAIL);
    expect(stripAnsi(txt)).toContain("Pre-release build");
  });

  test("noColor output: plain ASCII, [PRE-RELEASE] tag, contact email", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { noColor: true });
    const txt = out.read();
    expect(txt).not.toMatch(/\x1b\[/);
    expect(txt).toContain(PRE_RELEASE_TAG);
    expect(txt).toContain(CONTACT_EMAIL);
    expect(txt).toMatch(/^\n/);
    expect(txt).toMatch(/\n\n$/);
  });
});

// ── printVersion (the new replacement for printBanner) ────────────────────────

describe("printVersion — T1: happy path", () => {
  test("color mode writes a single-line version string", () => {
    const out = makeTtyBufferStream();
    printVersion(out.stream, VERSION, {});
    const lines = out.read().split("\n").filter((l) => l !== "");
    expect(lines.length).toBe(1);
    expect(stripAnsi(lines[0])).toContain(`zombiectl`);
    expect(stripAnsi(lines[0])).toContain(`v${VERSION}`);
  });

  test("noColor mode writes the exact plain version line", () => {
    const out = makeBufferStream();
    printVersion(out.stream, VERSION, { noColor: true });
    expect(out.read()).toBe(`zombiectl v${VERSION}\n`);
  });

  test("jsonMode suppresses all output", () => {
    const out = makeBufferStream();
    printVersion(out.stream, VERSION, { jsonMode: true });
    expect(out.read()).toBe("");
  });
});

describe("printVersion — T2: edge cases", () => {
  test("empty version still writes", () => {
    const out = makeBufferStream();
    printVersion(out.stream, "", { noColor: true });
    expect(out.read()).toBe("zombiectl v\n");
  });

  test("semver pre-release version preserved", () => {
    const out = makeBufferStream();
    printVersion(out.stream, "1.2.3-beta.1", { noColor: true });
    expect(out.read()).toContain("1.2.3-beta.1");
  });

  test("no opts argument uses defaults (TTY → color, non-TTY → plain)", () => {
    const tty = makeTtyBufferStream();
    printVersion(tty.stream, "0.3.1");
    expect(tty.read().length).toBeGreaterThan(0);

    const plain = makeBufferStream();
    printVersion(plain.stream, "0.3.1");
    expect(plain.read().length).toBeGreaterThan(0);
  });
});

describe("printVersion — T7: design-system regression guards", () => {
  test("color mode output contains no decorative ASCII art", () => {
    const out = makeTtyBufferStream();
    printVersion(out.stream, VERSION, {});
    const txt = out.read();
    for (const ch of FORBIDDEN_BANNER_CHARS) {
      expect(txt).not.toContain(ch);
    }
  });

  test("noColor mode output contains no decorative ASCII art", () => {
    const out = makeBufferStream();
    printVersion(out.stream, VERSION, { noColor: true });
    const txt = out.read();
    for (const ch of FORBIDDEN_BANNER_CHARS) {
      expect(txt).not.toContain(ch);
    }
  });

  test("color mode output contains pulse-cyan dot glyph", () => {
    const out = makeTtyBufferStream();
    printVersion(out.stream, VERSION, {});
    expect(out.read()).toContain("●");
  });

  test("color mode includes the 256-color pulse-cyan code (79)", () => {
    const out = makeTtyBufferStream();
    printVersion(out.stream, VERSION, {});
    expect(out.read()).toContain("38;5;79");
  });

  test("does not advertise itself as 'autonomous agent cli'", () => {
    // The previous banner printed an "autonomous agent cli" subtitle. The
    // design system retired the subtitle; --version is one line.
    const out = makeTtyBufferStream();
    printVersion(out.stream, VERSION, {});
    expect(stripAnsi(out.read())).not.toContain("autonomous agent cli");
  });

  test("noColor output is exactly one line", () => {
    const out = makeBufferStream();
    printVersion(out.stream, VERSION, { noColor: true });
    const lines = out.read().split("\n").filter((l) => l !== "");
    expect(lines.length).toBe(1);
  });
});

// ── VERSION constant + ttyOnly integration ─────────────────────────────────────

describe("VERSION — T7 + T12: constant matches package.json", () => {
  test("VERSION exported from cli.js matches package.json version", () => {
    const pkg = JSON.parse(
      readFileSync(new URL("../package.json", import.meta.url), "utf8"),
    );
    expect(VERSION).toBe(pkg.version);
  });

  test("VERSION is a valid semver string", () => {
    expect(VERSION).toMatch(/^\d+\.\d+\.\d+$/);
  });
});

describe("ttyOnly flag — T6: integration via runCli", () => {
  test("pre-release warning shown when stderr is a TTY", async () => {
    const out = makeBufferStream();
    const err = makeTtyBufferStream();
    const code = await runCli(["--version"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1" },
    });
    expect(code).toBe(0);
    expect(err.read()).toContain("[PRE-RELEASE]");
  });

  test("pre-release warning suppressed when stderr is not a TTY", async () => {
    const out = makeBufferStream();
    const err = makeBufferStream();
    const code = await runCli(["--version"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1" },
    });
    expect(code).toBe(0);
    expect(err.read()).toBe("");
  });

  test("pre-release warning suppressed in --json mode even on TTY stderr", async () => {
    const out = makeBufferStream();
    const err = makeTtyBufferStream();
    const code = await runCli(["--json", "--version"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env },
    });
    expect(code).toBe(0);
    expect(err.read()).toBe("");
  });

  test("--version stdout matches in TTY and non-TTY paths", async () => {
    const out1 = makeBufferStream();
    await runCli(["--version"], {
      stdout: out1.stream,
      stderr: makeBufferStream().stream,
      env: { NO_COLOR: "1" },
    });
    const out2 = makeBufferStream();
    await runCli(["--version"], {
      stdout: out2.stream,
      stderr: makeTtyBufferStream().stream,
      env: { NO_COLOR: "1" },
    });
    expect(out1.read()).toContain(`zombiectl v${VERSION}`);
    expect(out2.read()).toContain(`zombiectl v${VERSION}`);
  });
});

describe("ttyOnly flag — T1 + T4: output fidelity via runCli", () => {
  test("--version --json stdout is parseable JSON with correct version", async () => {
    const out = makeBufferStream();
    await runCli(["--json", "--version"], {
      stdout: out.stream,
      stderr: makeBufferStream().stream,
      env: { ...process.env },
    });
    const parsed = JSON.parse(out.read());
    expect(parsed.version).toBe(VERSION);
  });

  test("--version NO_COLOR output has no ANSI on stdout", async () => {
    const out = makeBufferStream();
    await runCli(["--version"], {
      stdout: out.stream,
      stderr: makeBufferStream().stream,
      env: { NO_COLOR: "1" },
    });
    expect(out.read()).not.toMatch(/\x1b\[/);
  });
});
