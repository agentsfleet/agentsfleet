/**
 * Unit tests for src/program/banner.js — printBanner and printPreReleaseWarning.
 *
 * Coverage tiers addressed:
 *   T1  Happy path
 *   T2  Edge cases
 *   T3  Negative / error paths (void functions — suppression paths covered here)
 *   T4  Output fidelity (actual rendered text and ANSI correctness)
 *   T5  Concurrency — N/A (pure write functions, no shared state)
 *   T6  Integration via runCli ttyOnly flag
 *   T7  Regression guards (email, date, version constants pinned)
 *   T8  Security — N/A (no user input, no secret handling)
 *   T9  DRY — shared helpers from helpers.js
 *   T10 Constants / magic values flagged
 *   T11 Performance — N/A (simple stream writes, no allocation concern)
 *   T12 CLI contract (--version output, --json suppression, VERSION matches package.json)
 */

import { describe, test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { makeBufferStream } from "./helpers.js";
import { printBanner, printPreReleaseWarning } from "../src/program/banner.js";
import { runCli, VERSION } from "../src/cli.js";

// ── T9: helpers — no magic, no copy-paste ─────────────────────────────────────

/** Simulate a TTY stderr by setting isTTY = true on the underlying stream. */
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
// If either of these breaks, update the source AND the tests together.
const CONTACT_EMAIL = "nkishore@megam.io";
const LAUNCH_DATE   = "April 5, 2026";
const PRE_RELEASE_TAG = "[PRE-RELEASE]";

// ── printPreReleaseWarning ─────────────────────────────────────────────────────

describe("printPreReleaseWarning — T1: happy path", () => {
  test("default opts writes non-empty output (color mode)", () => {
    const out = makeBufferStream();
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

describe("printPreReleaseWarning — T2: edge cases", () => {
  test("no opts argument uses defaults (color mode)", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream);
    expect(out.read().length).toBeGreaterThan(0);
  });

  test("empty opts object uses defaults (color mode)", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, {});
    expect(out.read().length).toBeGreaterThan(0);
  });

  test("noColor=false produces colored output (same as default)", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { noColor: false });
    expect(out.read()).toMatch(/\x1b\[/);
  });

  test("jsonMode=false + noColor=false produces colored output", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { jsonMode: false, noColor: false });
    expect(out.read()).toMatch(/\x1b\[/);
  });

  test("ttyOnly=true suppresses even when noColor=false", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { ttyOnly: true, noColor: false });
    expect(out.read()).toBe("");
  });

  test("jsonMode=true takes priority over noColor=true (both suppress-eligible)", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { jsonMode: true, noColor: true });
    expect(out.read()).toBe("");
  });

  test("ttyOnly=true + jsonMode=true both result in empty output", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { ttyOnly: true, jsonMode: true });
    expect(out.read()).toBe("");
  });
});

describe("printPreReleaseWarning — T3: suppression (negative) paths", () => {
  test("jsonMode suppresses output regardless of noColor value", () => {
    for (const noColor of [true, false]) {
      const out = makeBufferStream();
      printPreReleaseWarning(out.stream, { jsonMode: true, noColor });
      expect(out.read()).toBe("");
    }
  });

  test("ttyOnly suppresses output regardless of noColor value", () => {
    for (const noColor of [true, false]) {
      const out = makeBufferStream();
      printPreReleaseWarning(out.stream, { ttyOnly: true, noColor });
      expect(out.read()).toBe("");
    }
  });
});

describe("printPreReleaseWarning — T4: output fidelity (color mode)", () => {
  test("color output contains ANSI escape codes", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, {});
    expect(out.read()).toMatch(/\x1b\[/);
  });

  test("color output contains contact email", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, {});
    expect(stripAnsi(out.read())).toContain(CONTACT_EMAIL);
  });

  test("color output contains launch date", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, {});
    expect(stripAnsi(out.read())).toContain(LAUNCH_DATE);
  });

  test("color output contains warning symbol", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, {});
    expect(out.read()).toContain("⚠");
  });

  test("color output starts with a newline", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, {});
    expect(out.read()).toMatch(/^\n/);
  });

  test("color output ends with double newline", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, {});
    expect(out.read()).toMatch(/\n\n$/);
  });

  test("color output mentions 'Pre-release build'", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, {});
    expect(stripAnsi(out.read())).toContain("Pre-release build");
  });
});

describe("printPreReleaseWarning — T4: output fidelity (noColor mode)", () => {
  test("noColor output contains no ANSI escape codes", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { noColor: true });
    expect(out.read()).not.toMatch(/\x1b\[/);
  });

  test("noColor output contains contact email", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { noColor: true });
    expect(out.read()).toContain(CONTACT_EMAIL);
  });

  test("noColor output contains launch date", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { noColor: true });
    expect(out.read()).toContain(LAUNCH_DATE);
  });

  test("noColor output contains [PRE-RELEASE] tag", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { noColor: true });
    expect(out.read()).toContain(PRE_RELEASE_TAG);
  });

  test("noColor output starts with newline", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { noColor: true });
    expect(out.read()).toMatch(/^\n/);
  });

  test("noColor output ends with double newline", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { noColor: true });
    expect(out.read()).toMatch(/\n\n$/);
  });
});

describe("printPreReleaseWarning — T7: regression guards", () => {
  test("contact email is nkishore@megam.io — pin against accidental change", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { noColor: true });
    expect(out.read()).toContain("nkishore@megam.io");
  });

  test("launch date is April 5, 2026 — pin against accidental change", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { noColor: true });
    expect(out.read()).toContain("April 5, 2026");
  });

  test("plain [PRE-RELEASE] tag present in noColor output — regression guard", () => {
    const out = makeBufferStream();
    printPreReleaseWarning(out.stream, { noColor: true });
    expect(out.read()).toContain("[PRE-RELEASE]");
  });

  test("contact email same in both color and noColor paths — not diverged", () => {
    const color = makeBufferStream();
    const plain = makeBufferStream();
    printPreReleaseWarning(color.stream, {});
    printPreReleaseWarning(plain.stream, { noColor: true });
    expect(stripAnsi(color.read())).toContain(CONTACT_EMAIL);
    expect(plain.read()).toContain(CONTACT_EMAIL);
  });

  test("launch date same in both color and noColor paths — not diverged", () => {
    const color = makeBufferStream();
    const plain = makeBufferStream();
    printPreReleaseWarning(color.stream, {});
    printPreReleaseWarning(plain.stream, { noColor: true });
    expect(stripAnsi(color.read())).toContain(LAUNCH_DATE);
    expect(plain.read()).toContain(LAUNCH_DATE);
  });
});

// ── printBanner ────────────────────────────────────────────────────────────────

describe("printBanner — T1: happy path", () => {
  test("color mode writes version to stream", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "0.3.1", {});
    expect(stripAnsi(out.read())).toContain("zombiectl v0.3.1");
  });

  test("noColor mode writes plain version line", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "0.3.1", { noColor: true });
    expect(out.read()).toBe("zombiectl v0.3.1\n");
  });

  test("jsonMode suppresses all output", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "0.3.1", { jsonMode: true });
    expect(out.read()).toBe("");
  });
});

describe("printBanner — T2: edge cases", () => {
  test("empty version string still writes", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "", { noColor: true });
    expect(out.read()).toBe("zombiectl v\n");
  });

  test("semver pre-release version string preserved", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "1.2.3-beta.1", { noColor: true });
    expect(out.read()).toContain("1.2.3-beta.1");
  });

  test("no opts argument uses defaults (color mode)", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "0.3.1");
    expect(out.read().length).toBeGreaterThan(0);
  });

  test("empty opts uses defaults (color mode)", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "0.3.1", {});
    expect(out.read().length).toBeGreaterThan(0);
  });
});

describe("printBanner — T4: output fidelity", () => {
  test("color mode output contains ANSI escape codes", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "0.3.1", {});
    expect(out.read()).toMatch(/\x1b\[/);
  });

  test("color mode output contains box-drawing character", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "0.3.1", {});
    expect(out.read()).toContain("\u2500"); // ─ horizontal bar
  });

  test("color mode output contains tagline 'autonomous agent cli'", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "0.3.1", {});
    expect(stripAnsi(out.read())).toContain("autonomous agent cli");
  });

  test("noColor mode output contains no ANSI codes", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "0.3.1", { noColor: true });
    expect(out.read()).not.toMatch(/\x1b\[/);
  });

  test("noColor mode output is exactly 'zombiectl v{version}\\n'", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "0.3.1", { noColor: true });
    expect(out.read()).toBe("zombiectl v0.3.1\n");
  });
});

describe("printBanner — T7: regression guards", () => {
  test("noColor plain format pinned — regression guard", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "0.3.1", { noColor: true });
    expect(out.read()).toBe("zombiectl v0.3.1\n");
  });

  test("color mode contains zombie emoji 🧟", () => {
    const out = makeBufferStream();
    printBanner(out.stream, "0.3.1", {});
    expect(out.read()).toContain("\u{1F9DF}");
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

  test("VERSION is 0.3.1", () => {
    expect(VERSION).toBe("0.3.1");
  });
});

describe("ttyOnly flag — T6: integration via runCli", () => {
  test("pre-release warning shown when stderr is a TTY", async () => {
    const out = makeBufferStream();
    const err = makeTtyBufferStream(); // isTTY = true
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
    const err = makeBufferStream(); // isTTY is undefined → non-TTY
    const code = await runCli(["--version"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1" },
    });
    expect(code).toBe(0);
    expect(err.read()).toBe(""); // clean — no banner, no errors
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

  test("--version stdout unaffected by ttyOnly flag (output always correct)", async () => {
    // Non-TTY path
    const out1 = makeBufferStream();
    await runCli(["--version"], {
      stdout: out1.stream,
      stderr: makeBufferStream().stream,
      env: { NO_COLOR: "1" },
    });
    // TTY path
    const out2 = makeBufferStream();
    await runCli(["--version"], {
      stdout: out2.stream,
      stderr: makeTtyBufferStream().stream,
      env: { NO_COLOR: "1" },
    });
    expect(out1.read()).toContain("zombiectl v0.3.1");
    expect(out2.read()).toContain("zombiectl v0.3.1");
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
