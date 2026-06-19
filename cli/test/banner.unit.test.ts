// Unit tests for src/program/banner.ts — printVersion.

import { describe, test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { makeBufferStream } from "./helpers.ts";
import { printVersion } from "../src/program/banner.ts";
import { runCli, VERSION } from "../src/cli.ts";

// ── helpers — no magic, no copy-paste ─────────────────────────────────────────

/** Simulate a TTY stream by setting isTTY = true on the underlying writable. */
function makeTtyBufferStream(): ReturnType<typeof makeBufferStream> {
  const b = makeBufferStream();
  b.stream.isTTY = true;
  return b;
}

/** Strip all ANSI escape sequences from a string for plain-text assertions. */
function stripAnsi(str: string): string {
  return str.replace(/\x1b\[[0-9;]*m/g, "");
}

// ── constants guard — pin strings that appear in both code paths ──────────────
const COLOR_ENV = { TERM: "xterm-256color" };

// Decorative-ASCII teardown — these MUST NOT appear in the version banner
// (per docs/DESIGN_SYSTEM.md "no decorative ASCII art"). Regression guards.
const FORBIDDEN_BANNER_CHARS = [
  "\u{1F9DF}", // 🧟 agent face
  "╭",    // ╭ box top-left
  "╮",    // ╮ box top-right
  "╰",    // ╰ box bottom-left
  "╯",    // ╯ box bottom-right
  "│",    // │ box vertical
];

// ── printVersion (the new replacement for printBanner) ────────────────────────

describe("printVersion — happy path", () => {
  test("color mode writes a single-line version string", () => {
    const out = makeTtyBufferStream();
    printVersion(out.stream, VERSION, {});
    const lines = out.read().split("\n").filter((l) => l !== "");
    expect(lines.length).toBe(1);
    expect(stripAnsi(lines[0] ?? "")).toContain(`agentsfleet`);
    expect(stripAnsi(lines[0] ?? "")).toContain(`v${VERSION}`);
  });

  test("noColor mode writes the exact plain version line", () => {
    const out = makeBufferStream();
    printVersion(out.stream, VERSION, { noColor: true });
    expect(out.read()).toBe(`agentsfleet v${VERSION}\n`);
  });

  test("jsonMode suppresses all output", () => {
    const out = makeBufferStream();
    printVersion(out.stream, VERSION, { jsonMode: true });
    expect(out.read()).toBe("");
  });
});

describe("printVersion — edge cases", () => {
  test("empty version still writes", () => {
    const out = makeBufferStream();
    printVersion(out.stream, "", { noColor: true });
    expect(out.read()).toBe("agentsfleet v\n");
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

describe("printVersion — design-system regression guards", () => {
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
    printVersion(out.stream, VERSION, { env: COLOR_ENV });
    expect(out.read()).toContain("●");
  });

  test("color mode includes the 256-color pulse-cyan code (79)", () => {
    // Pin TERM so capability detection deterministically returns xterm256
    // regardless of CI environment (where TERM may be missing or 'dumb').
    const out = makeTtyBufferStream();
    printVersion(out.stream, VERSION, { env: COLOR_ENV });
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

describe("VERSION — constant matches package.json", () => {
  test("VERSION exported from cli.ts matches package.json version", () => {
    const pkg = JSON.parse(
      readFileSync(new URL("../package.json", import.meta.url), "utf8"),
    );
    expect(VERSION).toBe(pkg.version);
  });

  test("VERSION is a valid semver string", () => {
    expect(VERSION).toMatch(/^\d+\.\d+\.\d+$/);
  });
});

describe("--version — integration via runCli", () => {
  test("stderr stays clean for --version (TTY)", async () => {
    const out = makeBufferStream();
    const err = makeTtyBufferStream();
    const code = await runCli(["--version"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1" },
    });
    expect(code).toBe(0);
    expect(err.read()).toBe("");
  });

  test("stderr stays clean for --version (non-TTY)", async () => {
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

  test("stderr stays clean in --json --version", async () => {
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
    expect(out1.read()).toContain(`agentsfleet v${VERSION}`);
    expect(out2.read()).toContain(`agentsfleet v${VERSION}`);
  });
});

describe("ttyOnly flag — output fidelity via runCli", () => {
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
