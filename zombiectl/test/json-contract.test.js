// M30_002 — CLI JSON Contract Standardization tests
//
// Dimensions covered:
//   1.1-1.2 Route inventory matches command registry
//   2.1     JSON mode suppresses banners and prose
//   2.3     JSON error shape is stable
//   3.1     runs interrupt route and JSON contract
//   3.2     Supported JSON commands parse with jq-equivalent
//   3.3     Supported JSON errors parse with jq-equivalent

import { describe, test, expect } from "bun:test";
import { makeBufferStream, makeNoop, ui, RUN_ID_1 } from "./helpers.js";
import { findRoute } from "../src/program/routes.js";
import { registerProgramCommands } from "../src/program/command-registry.js";
import { runCli } from "../src/cli.js";
import { commandRuns } from "../src/commands/runs.js";
import { commandRunsInterrupt } from "../src/commands/run_interrupt.js";
import { commandSpecInit } from "../src/commands/spec_init.js";
import { writeError } from "../src/program/io.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

function tryParseJson(str) {
  try {
    return JSON.parse(str.trim());
  } catch {
    return null;
  }
}

function makeRunsDeps(overrides = {}) {
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
    { cmd: "specs", args: ["sync"], key: "specs.sync" },
    { cmd: "spec", args: ["init"], key: "spec.init" },
    { cmd: "run", args: [], key: "run" },
    { cmd: "runs", args: ["list"], key: "runs.list" },
    { cmd: "runs", args: ["cancel"], key: "runs.cancel" },
    { cmd: "runs", args: ["replay"], key: "runs.replay" },
    { cmd: "runs", args: ["interrupt"], key: "runs.interrupt" },
    { cmd: "doctor", args: [], key: "doctor" },
    { cmd: "skill-secret", args: ["put"], key: "skill-secret" },
    { cmd: "admin", args: ["config"], key: "admin" },
  ];

  for (const { cmd, args, key } of expectedRoutes) {
    test(`route "${cmd} ${args.join(" ")}".trim() resolves to "${key}"`, () => {
      const route = findRoute(cmd, args);
      expect(route).not.toBeNull();
      expect(route.key).toBe(key);
    });
  }

  test("all route keys have matching registry handlers", () => {
    const noop = () => {};
    const handlers = registerProgramCommands({
      login: noop, logout: noop, workspace: noop,
      specsSync: noop, specInit: noop, run: noop,
      runsList: noop, runsCancel: noop, runsReplay: noop,
      runsInterrupt: noop, doctor: noop,
      skillSecret: noop, admin: noop,
    });
    for (const { key } of expectedRoutes) {
      expect(handlers[key]).toBeDefined();
    }
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
});

// ═══════════════════════════════════════════════════════════════════════
// §3.1 — runs interrupt route and JSON contract
// ═══════════════════════════════════════════════════════════════════════

describe("M30 §3.1 — runs interrupt is routed and JSON-capable", () => {
  test("runs interrupt route is registered", () => {
    const route = findRoute("runs", ["interrupt"]);
    expect(route).not.toBeNull();
    expect(route.key).toBe("runs.interrupt");
  });

  test("runs interrupt JSON success is parseable", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const ctx = {
      stdout, stderr: makeNoop(), jsonMode: true,
      token: "tok", apiKey: null, apiUrl: "http://localhost:3000", env: {},
    };
    const code = await commandRunsInterrupt(ctx, [RUN_ID_1, "fix", "auth"], makeRunsDeps());
    expect(code).toBe(0);
    const parsed = tryParseJson(read());
    expect(parsed).not.toBeNull();
    expect(parsed.ack).toBe(true);
  });

  test("runs interrupt JSON usage error is parseable", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = {
      stdout: makeNoop(), stderr, jsonMode: true,
      token: "tok", apiKey: null, apiUrl: "http://localhost:3000", env: {},
    };
    const code = await commandRunsInterrupt(ctx, [], makeRunsDeps());
    expect(code).toBe(2);
    const parsed = tryParseJson(read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("USAGE_ERROR");
  });
});

// ═══════════════════════════════════════════════════════════════════════
// §3.2-3.3 — runs cancel/replay JSON errors are parseable
// ═══════════════════════════════════════════════════════════════════════

describe("M30 §3.2-3.3 — runs subcommand JSON errors", () => {
  test("runs cancel missing run_id in JSON mode emits structured error", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = {
      stdout: makeNoop(), stderr, jsonMode: true,
      token: "tok", apiKey: null, apiUrl: "http://localhost:3000", env: {},
    };
    const code = await commandRuns(ctx, ["cancel"], makeRunsDeps());
    expect(code).toBe(2);
    const parsed = tryParseJson(read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("USAGE_ERROR");
  });

  test("runs replay missing run_id in JSON mode emits structured error", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = {
      stdout: makeNoop(), stderr, jsonMode: true,
      token: "tok", apiKey: null, apiUrl: "http://localhost:3000", env: {},
    };
    const code = await commandRuns(ctx, ["replay"], makeRunsDeps());
    expect(code).toBe(2);
    const parsed = tryParseJson(read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("USAGE_ERROR");
  });

  test("runs unknown subcommand in JSON mode emits structured error", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = {
      stdout: makeNoop(), stderr, jsonMode: true,
      token: "tok", apiKey: null, apiUrl: "http://localhost:3000", env: {},
    };
    const code = await commandRuns(ctx, ["bogus"], makeRunsDeps());
    expect(code).toBe(2);
    const parsed = tryParseJson(read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("UNKNOWN_COMMAND");
  });

  test("runs interrupt invalid mode in JSON mode emits structured error", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = {
      stdout: makeNoop(), stderr, jsonMode: true,
      token: "tok", apiKey: null, apiUrl: "http://localhost:3000", env: {},
    };
    const code = await commandRunsInterrupt(ctx, [RUN_ID_1, "msg", "--mode=turbo"], makeRunsDeps());
    expect(code).toBe(2);
    const parsed = tryParseJson(read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("USAGE_ERROR");
  });

  test("non-JSON error paths remain unchanged", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = {
      stdout: makeNoop(), stderr, jsonMode: false,
      token: "tok", apiKey: null, apiUrl: "http://localhost:3000", env: {},
    };
    const code = await commandRuns(ctx, ["cancel"], makeRunsDeps());
    expect(code).toBe(2);
    expect(read()).toContain("requires");
    expect(tryParseJson(read())).toBeNull();
  });
});

// ═══════════════════════════════════════════════════════════════════════
// §3.4 — writeError helper contract
// ═══════════════════════════════════════════════════════════════════════

describe("M30 §3.4 — writeError helper", () => {
  test("JSON mode emits structured error on stderr", () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = { jsonMode: true, stderr };
    writeError(ctx, "TEST_CODE", "test message", makeRunsDeps());
    const parsed = tryParseJson(read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("TEST_CODE");
    expect(parsed.error.message).toBe("test message");
  });

  test("non-JSON mode emits human text via ui.err", () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = { jsonMode: false, stderr };
    writeError(ctx, "TEST_CODE", "test message", makeRunsDeps());
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

// ═══════════════════════════════════════════════════════════════════════
// §3.6 — spec init JSON error paths (API_ERROR contract)
// ═══════════════════════════════════════════════════════════════════════

describe("M30 §3.6 — spec init JSON error contract", () => {
  test("spec init IO failure emits API_ERROR in JSON mode", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = {
      stdout: makeNoop(), stderr, jsonMode: true,
      token: null, apiKey: null, apiUrl: "http://localhost:3000", env: {},
    };
    // /dev/null is a special file — mkdirSync on a path beneath it must fail
    const code = await commandSpecInit(
      ["--path=.", "--output=/dev/null/nested/out.md"],
      ctx,
      makeRunsDeps()
    );
    expect(code).toBe(1);
    const parsed = tryParseJson(read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("API_ERROR");
  });

  test("spec init path-not-found emits USAGE_ERROR in JSON mode", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = {
      stdout: makeNoop(), stderr, jsonMode: true,
      token: null, apiKey: null, apiUrl: "http://localhost:3000", env: {},
    };
    const code = await commandSpecInit(
      ["--path=/nonexistent-path-zombie-test"],
      ctx,
      makeRunsDeps()
    );
    expect(code).toBe(2);
    const parsed = tryParseJson(read());
    expect(parsed).not.toBeNull();
    expect(parsed.error.code).toBe("USAGE_ERROR");
  });
});
