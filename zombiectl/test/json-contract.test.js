// M30_002 — CLI JSON Contract Standardization tests
//
// Dimensions covered:
//   1.1-1.2 Route inventory matches command registry
//   2.1     JSON mode suppresses banners and prose
//   2.3     JSON error shape is stable

import { describe, test, expect } from "bun:test";
import { makeBufferStream, ui } from "./helpers.js";
import { findRoute } from "../src/program/routes.js";
import { registerProgramCommands } from "../src/program/command-registry.js";
import { runCli } from "../src/cli.js";
import { writeError } from "../src/program/io.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

function tryParseJson(str) {
  try {
    return JSON.parse(str.trim());
  } catch {
    return null;
  }
}

function makeDeps(overrides = {}) {
  return {
    parseFlags: (tokens) => {
      const flags = {};
      const positionals = [];
      for (let i = 0; i < tokens.length; i++) {
        if (tokens[i].startsWith("--")) {
          const [k, v] = tokens[i].slice(2).split("=");
          flags[k] = v || true;
          continue;
        }
        positionals.push(tokens[i]);
      }
      return { flags, positionals, options: flags };
    },
    printJson: (s, v) => { s.write(JSON.stringify(v, null, 2) + "\n"); },
    request: async () => ({ ack: true, mode: "queued", request_id: "req-1" }),
    apiHeaders: () => ({ Authorization: "Bearer tok" }),
    ui,
    writeLine: (stream, line = "") => { stream.write((line ?? "") + "\n"); },
    ...overrides,
  };
}

// ═══════════════════════════════════════════════════════════════════════
// §1 — Route Inventory
// ═══════════════════════════════════════════════════════════════════════

describe("M30 §1 — route inventory matches command registry", () => {
  const expectedRoutes = [
    { cmd: "login", args: [], key: "login" },
    { cmd: "logout", args: [], key: "logout" },
    { cmd: "workspace", args: ["add"], key: "workspace" },
    { cmd: "doctor", args: [], key: "doctor" },
    { cmd: "admin", args: ["config"], key: "admin" },
    { cmd: "agent", args: ["create"], key: "agent" },
    { cmd: "grant", args: ["list"], key: "grant" },
    { cmd: "install", args: [], key: "zombie.install" },
    { cmd: "status", args: [], key: "zombie.status" },
    { cmd: "kill", args: [], key: "zombie.kill" },
    { cmd: "logs", args: [], key: "zombie.logs" },
    { cmd: "credential", args: [], key: "zombie.credential" },
  ];

  for (const { cmd, args, key } of expectedRoutes) {
    test(`route "${cmd} ${args.join(" ")}".trim() resolves to "${key}"`, () => {
      const route = findRoute(cmd, args);
      expect(route).not.toBeNull();
      expect(route.key).toBe(key);
    });
  }

  test("every registered route has a matching registry handler", () => {
    // Closes the gap where a route is declared in routes.js but the command
    // registry silently drops the handler — a previous greptile catch that
    // let `zombiectl agent/grant/install/...` fall through to UNKNOWN_COMMAND.
    const sentinel = () => "handled";
    const handlers = registerProgramCommands({
      login: sentinel,
      logout: sentinel,
      workspace: sentinel,
      doctor: sentinel,
      admin: sentinel,
      agent: sentinel,
      grant: sentinel,
      zombieInstall: sentinel,
      zombieStatus: sentinel,
      zombieKill: sentinel,
      zombieLogs: sentinel,
      zombieCredential: sentinel,
    });
    for (const { key } of expectedRoutes) {
      expect(handlers[key]).toBe(sentinel);
    }
  });

  test("removed v1 spec/run routes do not resolve", () => {
    expect(findRoute("run", [])).toBeNull();
    expect(findRoute("runs", ["list"])).toBeNull();
    expect(findRoute("runs", ["cancel"])).toBeNull();
    expect(findRoute("runs", ["replay"])).toBeNull();
    expect(findRoute("runs", ["interrupt"])).toBeNull();
    expect(findRoute("spec", ["init"])).toBeNull();
    expect(findRoute("specs", ["sync"])).toBeNull();
  });
});

// ═══════════════════════════════════════════════════════════════════════
// §2.1 — JSON mode suppresses banner/prose
// ═══════════════════════════════════════════════════════════════════════

describe("M30 §2.1 — JSON mode suppresses banners", () => {
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
    const parsed = tryParseJson(out.read());
    expect(parsed).not.toBeNull();
    expect(parsed.version).toBeDefined();
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
// §2.3 — JSON error shape is stable
// ═══════════════════════════════════════════════════════════════════════

describe("M30 §2.3 — JSON error shape", () => {
  test("unknown command in JSON mode emits structured error on stderr", async () => {
    const out = makeBufferStream();
    const err = makeBufferStream();
    const code = await runCli(["--json", "nonexistent"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" },
    });
    expect(code).toBe(2);
    const parsed = tryParseJson(err.read());
    expect(parsed).not.toBeNull();
    expect(parsed.error).toBeDefined();
    expect(parsed.error.code).toBe("UNKNOWN_COMMAND");
    expect(parsed.error.message).toContain("nonexistent");
  });

  test("auth required in JSON mode emits AUTH_REQUIRED on stderr", async () => {
    const out = makeBufferStream();
    const err = makeBufferStream();
    const code = await runCli(["--json", "workspace", "list"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1" },
    });
    expect(code).toBe(1);
    const parsed = tryParseJson(err.read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("AUTH_REQUIRED");
  });

  test("removed v1 spec/run commands emit UNKNOWN_COMMAND in JSON mode", async () => {
    for (const argv of [["--json", "run"], ["--json", "runs", "list"], ["--json", "spec", "init"]]) {
      const out = makeBufferStream();
      const err = makeBufferStream();
      const code = await runCli(argv, {
        stdout: out.stream,
        stderr: err.stream,
        env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" },
      });
      expect(code).toBe(2);
      const parsed = tryParseJson(err.read());
      expect(parsed).not.toBeNull();
      expect(parsed.error.code).toBe("UNKNOWN_COMMAND");
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════
// §3.4 — writeError helper contract
// ═══════════════════════════════════════════════════════════════════════

describe("M30 §3.4 — writeError helper", () => {
  test("JSON mode emits structured error on stderr", () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = { jsonMode: true, stderr };
    writeError(ctx, "TEST_CODE", "test message", makeDeps());
    const parsed = tryParseJson(read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("TEST_CODE");
    expect(parsed.error.message).toBe("test message");
  });

  test("non-JSON mode emits human text via ui.err", () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = { jsonMode: false, stderr };
    writeError(ctx, "TEST_CODE", "test message", makeDeps());
    expect(read()).toContain("test message");
    expect(tryParseJson(read())).toBeNull();
  });
});

// ═══════════════════════════════════════════════════════════════════════
// §3.5 — all command groups emit JSON errors for usage failures
// ═══════════════════════════════════════════════════════════════════════

describe("M30 §3.5 — workspace JSON errors", () => {
  test("workspace unknown subcommand emits UNKNOWN_COMMAND", async () => {
    const out = makeBufferStream();
    const err = makeBufferStream();
    const code = await runCli(["--json", "workspace", "bogus"], {
      stdout: out.stream, stderr: err.stream,
      env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" },
    });
    expect(code).toBe(2);
    const parsed = tryParseJson(err.read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("UNKNOWN_COMMAND");
  });

  test("workspace add missing repo_url emits USAGE_ERROR", async () => {
    const out = makeBufferStream();
    const err = makeBufferStream();
    const code = await runCli(["--json", "workspace", "add"], {
      stdout: out.stream, stderr: err.stream,
      env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" },
    });
    expect(code).toBe(2);
    const parsed = tryParseJson(err.read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("USAGE_ERROR");
  });

  test("workspace remove missing id emits USAGE_ERROR", async () => {
    const out = makeBufferStream();
    const err = makeBufferStream();
    const code = await runCli(["--json", "workspace", "remove"], {
      stdout: out.stream, stderr: err.stream,
      env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" },
    });
    expect(code).toBe(2);
    const parsed = tryParseJson(err.read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("USAGE_ERROR");
  });
});

describe("M30 §3.5 — admin JSON errors", () => {
  test("admin unknown subcommand emits UNKNOWN_COMMAND", async () => {
    const out = makeBufferStream();
    const err = makeBufferStream();
    const code = await runCli(["--json", "admin", "bogus"], {
      stdout: out.stream, stderr: err.stream,
      env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" },
    });
    expect(code).toBe(2);
    const parsed = tryParseJson(err.read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("UNKNOWN_COMMAND");
  });
});
