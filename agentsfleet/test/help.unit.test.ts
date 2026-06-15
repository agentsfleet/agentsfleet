import { describe, test, expect, beforeEach } from "bun:test";
import { AgentHelp, styleTagline } from "../src/program/help.ts";
import { formatHelpHeading, palette, resetCapabilityWarning } from "../src/output/index.ts";

// Three stream shims — one per ColorMode. detectColorMode resolves
// mode from {env, stream.isTTY}, so the test injects both. Real
// process.stdout is left untouched.
const TTY_STREAM = { isTTY: true, write: () => {} };
const NON_TTY_STREAM = { isTTY: false, write: () => {} };

const XTERM_ENV = { TERM: "xterm-256color" };
const BASIC_ENV = { TERM: "xterm" };
const NO_COLOR_ENV = { NO_COLOR: "1", TERM: "xterm-256color" };

beforeEach(() => {
  resetCapabilityWarning();
});

describe("AgentHelp.styleTitle", () => {
  test("bold pulse-cyan xterm256 ANSI under a 256-color TTY", () => {
    const help = new AgentHelp({ stream: TTY_STREAM, env: XTERM_ENV });
    const styled = help.styleTitle("USAGE");
    expect(styled).toBe(formatHelpHeading("USAGE", { stream: TTY_STREAM, env: XTERM_ENV }));
    // xterm256 wraps with `\x1b[1;38;5;79m...\x1b[0m` — pulse=79.
    expect(styled).toContain("\x1b[1;38;5;79m");
    expect(styled).toContain("\x1b[0m");
  });

  test("bold cyan basic16 ANSI under a non-256 TTY", () => {
    const help = new AgentHelp({ stream: TTY_STREAM, env: BASIC_ENV });
    const styled = help.styleTitle("USER COMMANDS");
    expect(styled).toBe(formatHelpHeading("USER COMMANDS", { stream: TTY_STREAM, env: BASIC_ENV }));
    // basic16 wraps with `\x1b[1;36m...\x1b[0m` — pulse → cyan (36).
    expect(styled).toContain("\x1b[1;36m");
    expect(styled).toContain("\x1b[0m");
  });

  test("plain text under NO_COLOR=1 — no ANSI escapes", () => {
    const help = new AgentHelp({ stream: TTY_STREAM, env: NO_COLOR_ENV });
    const styled = help.styleTitle("GLOBAL FLAGS");
    expect(styled).toBe("GLOBAL FLAGS");
    expect(styled).not.toMatch(/\x1b\[/);
  });

  test("plain text under !isTTY without FORCE_COLOR", () => {
    const help = new AgentHelp({ stream: NON_TTY_STREAM, env: XTERM_ENV });
    const styled = help.styleTitle("AGENT COMMANDS");
    expect(styled).toBe("AGENT COMMANDS");
    expect(styled).not.toMatch(/\x1b\[/);
  });

  test("FORCE_COLOR=2 lifts non-TTY streams back to xterm256", () => {
    const env = { FORCE_COLOR: "2", TERM: "dumb" };
    const help = new AgentHelp({ stream: NON_TTY_STREAM, env });
    const styled = help.styleTitle("BILLING COMMANDS");
    expect(styled).toContain("\x1b[1;38;5;79m");
  });

  test("constructor defaults to process.stdout + process.env when unset", () => {
    const help = new AgentHelp();
    // Reach into the private resolved opts via a typed unwrap. The defaults
    // contract is part of the test surface even though styleOpts is private.
    const opts = (help as unknown as { styleOpts: { stream: NodeJS.WritableStream; env: NodeJS.ProcessEnv } }).styleOpts;
    expect(opts.stream).toBe(process.stdout);
    expect(opts.env).toBe(process.env);
  });
});

describe("styleTagline", () => {
  test("subtle grey 244 under a 256-color TTY", () => {
    const styled = styleTagline("usezombie cli", {
      stream: TTY_STREAM,
      env: XTERM_ENV,
    });
    expect(styled).toBe(palette.subtle("usezombie cli", {
      stream: TTY_STREAM,
      env: XTERM_ENV,
    }));
    // subtle=244 in xterm256.
    expect(styled).toContain("\x1b[38;5;244m");
  });

  test("basic16 dim under a non-256 TTY", () => {
    const styled = styleTagline("usezombie cli", {
      stream: TTY_STREAM,
      env: BASIC_ENV,
    });
    // subtle → dim (SGR 2) in basic16.
    expect(styled).toContain("\x1b[2m");
  });

  test("plain text under NO_COLOR=1", () => {
    const styled = styleTagline("usezombie cli", {
      stream: TTY_STREAM,
      env: NO_COLOR_ENV,
    });
    expect(styled).toBe("usezombie cli");
  });

  test("defaults to process.stdout + process.env when opts omitted", () => {
    // No assertion on the styling — runtime-dependent. The contract is
    // "doesn't throw, returns a string containing the input".
    const styled = styleTagline("usezombie cli");
    expect(typeof styled).toBe("string");
    expect(styled).toContain("usezombie cli");
  });
});
