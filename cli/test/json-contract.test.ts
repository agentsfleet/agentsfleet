// CLI JSON contract — every documented command resolves in the tree, JSON
// mode suppresses banners/prose, the JSON error envelope shape is stable, and
// removed v1 routes (run/runs/spec/specs) surface as unknown instead of
// resolving silently.

import { describe, test, expect } from "bun:test";
import { mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { makeBufferStream, ui } from "./helpers.ts";
import { runCli } from "../src/cli.ts";
import { EXIT_CODE } from "../src/errors/index.ts";
import { STATE_DIR_ENV } from "../src/lib/config-dir.ts";
import { writeError } from "../src/program/io.ts";
import { rootCommand } from "../src/program/tree/root.command.ts";
import { childrenOf, type CommandNode } from "../src/program/tree/resolve-path.ts";

// The credential store resolves from the environment `runCli` is handed, so
// the auth-shaped cases below isolate by passing this directory in `io.env` —
// no process-environment mutation, and a developer's real login never reaches
// an assertion about unauthenticated behaviour.
const isolatedStateDir = mkdtempSync(path.join(os.tmpdir(), "agentsfleet-json-contract-"));

function tryParseJson(str: string): unknown {
  try {
    return JSON.parse(str.trim());
  } catch {
    return null;
  }
}

function findSubcommand(...names: ReadonlyArray<string>): CommandNode | null {
  let node = rootCommand as unknown as CommandNode;
  for (const name of names) {
    const next = childrenOf(node).find((child) => child.name === name);
    if (!next) return null;
    node = next;
  }
  return node;
}

// ═══════════════════════════════════════════════════════════════════════
// Command tree exposes every documented route
// ═══════════════════════════════════════════════════════════════════════

describe("CLI tree — every documented route is reachable", () => {
  const expectedCommands = [
    ["login"], ["logout"], ["doctor"],
    ["workspace", "create"], ["workspace", "list"], ["workspace", "use"],
    ["workspace", "show"], ["workspace", "secrets"], ["workspace", "delete"],
    ["api-key", "create"], ["api-key", "list"], ["api-key", "revoke"], ["api-key", "delete"],
    ["connector", "list"], ["connector", "status"],
    ["grant", "list"], ["grant", "delete"],
    ["schedule", "add"], ["schedule", "list"], ["schedule", "update"], ["schedule", "rm"], ["schedule", "status"], ["schedule", "sync"],
    ["tenant", "provider", "show"], ["tenant", "provider", "create"], ["tenant", "provider", "delete"],
    ["billing", "show"],
    ["install"], ["list"], ["status"], ["stop"], ["resume"], ["kill"], ["delete"],
    ["logs"], ["events"], ["steer"],
    ["secret", "create"], ["secret", "update"], ["secret", "show"], ["secret", "list"], ["secret", "delete"],
  ];

  for (const path of expectedCommands) {
    test(`the tree resolves "${path.join(" ")}"`, () => {
      const node = findSubcommand(...path);
      expect(node).not.toBeNull();
      // A leaf is a command someone can run; a group would resolve by name
      // while having nothing to execute.
      expect(childrenOf(node as CommandNode)).toEqual([]);
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
      // The injected environment reaches the credential store, so an empty
      // isolated directory here IS the unauthenticated state — no process
      // scope guard needed.
      env: { NO_COLOR: "1", [STATE_DIR_ENV]: isolatedStateDir },
    });
    expect(code).toBe(1);
    const parsed = tryParseJson(err.read()) as { error: { code: string } } | null;
    expect(parsed).not.toBeNull();
    expect(parsed?.error.code).toBe("AUTH_REQUIRED");
  });

  test("removed v1 commands surface as unknown (validation exit)", async () => {
    for (const argv of [["run"], ["runs", "list"], ["spec", "init"]]) {
      const out = makeBufferStream();
      const err = makeBufferStream();
      const code = await runCli(argv, {
        stdout: out.stream,
        stderr: err.stream,
        env: { ...process.env, AGENTSFLEET_API_KEY: "agt_t_test" },
      });
      expect(code).toBe(EXIT_CODE.ValidationError);
      expect(err.read()).toMatch(/Unknown subcommand/);
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
