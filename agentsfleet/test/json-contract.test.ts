// CLI JSON contract — every documented command resolves through
// commander, JSON mode suppresses banners/prose, the JSON error
// envelope shape is stable, and removed v1 routes (run/runs/spec/specs)
// surface UNKNOWN_COMMAND instead of resolving silently.

import { describe, test, expect } from "bun:test";
import type { Command } from "commander";
import { makeBufferStream, ui } from "./helpers.ts";
import { runCli } from "../src/cli.ts";
import { writeError } from "../src/program/io.ts";
import { buildProgram } from "../src/program/cli-tree.ts";
import type { CommandHandlerFn, Handlers } from "../src/program/cli-tree-types.ts";

function tryParseJson(str: string): unknown {
  try {
    return JSON.parse(str.trim());
  } catch {
    return null;
  }
}

function makeStubHandlers(): Handlers {
  const noop: CommandHandlerFn = async () => 0;
  return {
    login: noop, logout: noop, doctor: noop,
    auth:      { status: noop },
    workspace: { add: noop, list: noop, use: noop, show: noop, credentials: noop, delete: noop },
    agent:     { add: noop, list: noop, delete: noop },
    grant:     { list: noop, delete: noop },
    tenant:    { provider: { show: noop, add: noop, delete: noop } },
    billing:   { show: noop },
    zombie: {
      install: noop, update: noop, list: noop, status: noop, stop: noop, resume: noop,
      kill: noop, delete: noop, logs: noop, events: noop, steer: noop,
      credential: { add: noop, show: noop, list: noop, delete: noop },
    },
    memory: { list: noop, search: noop },
  };
}

function findSubcommand(program: Command, ...names: string[]): Command | null {
  let cmd: Command = program;
  for (const name of names) {
    const next: Command | undefined = cmd.commands.find((c) => c.name() === name);
    if (!next) return null;
    cmd = next;
  }
  return cmd;
}

// ═══════════════════════════════════════════════════════════════════════
// Command tree exposes every documented route
// ═══════════════════════════════════════════════════════════════════════

describe("CLI tree — every documented route is reachable through commander", () => {
  const program = buildProgram({
    handlers: makeStubHandlers(),
    version: "0.0.0",
    state: { exitCode: 0 },
  });

  const expectedCommands = [
    ["login"], ["logout"], ["doctor"],
    ["workspace", "add"], ["workspace", "list"], ["workspace", "use"],
    ["workspace", "show"], ["workspace", "credentials"], ["workspace", "delete"],
    ["agent", "add"], ["agent", "list"], ["agent", "delete"],
    ["grant", "list"], ["grant", "delete"],
    ["tenant", "provider", "show"], ["tenant", "provider", "add"], ["tenant", "provider", "delete"],
    ["billing", "show"],
    ["install"], ["list"], ["status"], ["stop"], ["resume"], ["kill"], ["delete"],
    ["logs"], ["events"], ["steer"],
    ["credential", "add"], ["credential", "show"], ["credential", "list"], ["credential", "delete"],
  ];

  for (const path of expectedCommands) {
    test(`commander tree resolves "${path.join(" ")}"`, () => {
      const cmd = findSubcommand(program, ...path);
      expect(cmd).not.toBeNull();
      const handler = (cmd as unknown as { _actionHandler?: unknown })._actionHandler;
      expect(typeof handler).toBe("function");
    });
  }
});

// ═══════════════════════════════════════════════════════════════════════
// JSON mode suppresses banner/prose
// ═══════════════════════════════════════════════════════════════════════

describe("JSON mode suppresses banners", () => {
  test("--json --version emits parseable JSON with no banner", async () => {
    const out = makeBufferStream();
    const err = makeBufferStream();
    const code = await runCli(["--json", "--version"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env },
    });
    expect(code).toBe(0);
    expect(err.read()).toBe("");
    const parsed = tryParseJson(out.read()) as { version?: string } | null;
    expect(parsed).not.toBeNull();
    expect(parsed?.version).toBeDefined();
  });

  test("--json --help emits no ANSI on stdout", async () => {
    const out = makeBufferStream();
    const err = makeBufferStream();
    const code = await runCli(["--json", "--help"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env },
    });
    expect(code).toBe(0);
    expect(out.read()).not.toMatch(/\x1b\[/);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// Auth + writeError envelope shapes
// ═══════════════════════════════════════════════════════════════════════

describe("JSON error envelope", () => {
  test("auth required in JSON mode emits AUTH_REQUIRED on stderr", async () => {
    const out = makeBufferStream();
    const err = makeBufferStream();
    const code = await runCli(["--json", "workspace", "list"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1" },
    });
    expect(code).toBe(1);
    const parsed = tryParseJson(err.read()) as { error: { code: string } } | null;
    expect(parsed).not.toBeNull();
    expect(parsed?.error.code).toBe("AUTH_REQUIRED");
  });

  test("removed v1 commands surface as commander unknown-command (exit 2)", async () => {
    for (const argv of [["run"], ["runs", "list"], ["spec", "init"]]) {
      const out = makeBufferStream();
      const err = makeBufferStream();
      const code = await runCli(argv, {
        stdout: out.stream,
        stderr: err.stream,
        env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" },
      });
      expect(code).toBe(2);
      expect(err.read()).toMatch(/unknown command/);
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════
// writeError helper contract
// ═══════════════════════════════════════════════════════════════════════

describe("writeError helper", () => {
  test("JSON mode emits structured error on stderr", () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = { jsonMode: true, stderr };
    writeError(ctx, "TEST_CODE", "test message", { ui });
    const parsed = tryParseJson(read()) as { error: { code: string; message: string } } | null;
    expect(parsed).not.toBeNull();
    expect(parsed?.error.code).toBe("TEST_CODE");
    expect(parsed?.error.message).toBe("test message");
  });

  test("non-JSON mode emits human text via ui.err", () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = { jsonMode: false, stderr };
    writeError(ctx, "TEST_CODE", "test message", { ui });
    expect(read()).toContain("test message");
    expect(tryParseJson(read())).toBeNull();
  });
});
