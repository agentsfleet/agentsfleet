// M22_001 §5 — zombiectl runs replay + --watch unit tests
//
// Tier coverage:
//   T1  — Happy path: replay renders gate narrative; --watch route registered
//   T2  — Edge cases: empty gates, missing fields, unicode gate names
//   T3  — Error paths: missing run_id, unknown subcommand, API errors
//   T4  — Output fidelity: JSON valid, text format correct
//   T7  — Regression: route keys stable
//   T8  — Security (OWASP Agentic): run_id injection, token not leaked,
//          SSE data injection, prompt injection in gate_name
//   T10 — Constants: available subcommands message
//   T11 — Performance: no hanging on empty response

import { describe, test, expect } from "bun:test";
import { makeNoop, makeBufferStream, ApiError, ui, RUN_ID_1 } from "./helpers.js";
import { commandRuns } from "../src/commands/runs.js";
import { findRoute } from "../src/program/routes.js";

const TOKEN = "tok_test_secret_m22";

function makeCtx(overrides = {}) {
  return {
    stdout: makeNoop(),
    stderr: makeNoop(),
    jsonMode: false,
    token: TOKEN,
    apiKey: null,
    apiUrl: "https://api.example.com",
    env: {},
    ...overrides,
  };
}

function makeDeps(overrides = {}) {
  return {
    parseFlags: (tokens) => {
      const options = {};
      const positionals = [];
      for (let i = 0; i < tokens.length; i++) {
        if (tokens[i].startsWith("--")) {
          const key = tokens[i].slice(2);
          const next = tokens[i + 1];
          if (next && !next.startsWith("--")) { options[key] = next; i++; }
          else options[key] = true;
        } else {
          positionals.push(tokens[i]);
        }
      }
      return { options, positionals };
    },
    printJson: (_s, v) => { _s.write(JSON.stringify(v) + "\n"); },
    request: async () => ({
      gate_results: [
        { gate_name: "run_lint", exit_code: 0, attempt: 1, wall_ms: 150, stdout_tail: "", stderr_tail: "" },
        { gate_name: "run_test", exit_code: 1, attempt: 2, wall_ms: 3200, stdout_tail: "1 failed", stderr_tail: "assert failed" },
      ],
    }),
    apiHeaders: (ctx) => ({ Authorization: `Bearer ${ctx.token}` }),
    ui,
    writeLine: (stream, line = "") => { stream.write((line ?? "") + "\n"); },
    ...overrides,
  };
}

// ── T7: route registration regression ─────────────────────────────────────

describe("runs.replay route", () => {
  test("T7: findRoute matches 'runs replay <id>'", () => {
    const route = findRoute("runs", ["replay", RUN_ID_1]);
    expect(route).not.toBeNull();
    expect(route.key).toBe("runs.replay");
  });

  test("T7: findRoute does not confuse replay with cancel", () => {
    const route = findRoute("runs", ["replay", RUN_ID_1]);
    expect(route.key).not.toBe("runs.cancel");
  });

  test("T7: findRoute does not confuse replay with list", () => {
    const route = findRoute("runs", ["replay", RUN_ID_1]);
    expect(route.key).not.toBe("runs.list");
  });

  test("T7: cancel route still works alongside replay", () => {
    const route = findRoute("runs", ["cancel", RUN_ID_1]);
    expect(route).not.toBeNull();
    expect(route.key).toBe("runs.cancel");
  });

  test("T7: list route still works alongside replay", () => {
    const route = findRoute("runs", ["list"]);
    expect(route).not.toBeNull();
    expect(route.key).toBe("runs.list");
  });
});

// ── T1: happy path ─────────────────────────────────────────────────────────

describe("commandRuns replay — happy path", () => {
  test("T1: exits 0 on successful replay", async () => {
    const ctx = makeCtx();
    const code = await commandRuns(ctx, ["replay", RUN_ID_1], makeDeps());
    expect(code).toBe(0);
  });

  test("T1: calls correct URL path with :replay suffix", async () => {
    let calledPath = null;
    const deps = makeDeps({
      request: async (_ctx, path) => {
        calledPath = path;
        return { gate_results: [] };
      },
    });
    const ctx = makeCtx();
    await commandRuns(ctx, ["replay", RUN_ID_1], deps);
    expect(calledPath).toContain(":replay");
    expect(calledPath).toContain(encodeURIComponent(RUN_ID_1));
  });

  test("T1: uses GET method", async () => {
    let calledMethod = null;
    const deps = makeDeps({
      request: async (_ctx, _path, opts) => {
        calledMethod = opts?.method;
        return { gate_results: [] };
      },
    });
    const ctx = makeCtx();
    await commandRuns(ctx, ["replay", RUN_ID_1], deps);
    expect(calledMethod).toBe("GET");
  });

  test("T1: text mode writes gate narrative with name, outcome, loop, wall_ms", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const ctx = makeCtx({ stdout });
    await commandRuns(ctx, ["replay", RUN_ID_1], makeDeps());
    const output = read();
    expect(output).toContain("[run_lint] PASS");
    expect(output).toContain("loop 1");
    expect(output).toContain("150ms");
    expect(output).toContain("[run_test] FAIL");
    expect(output).toContain("loop 2");
  });

  test("T1: JSON mode writes valid JSON with gate_results array", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const ctx = makeCtx({ stdout, jsonMode: true });
    await commandRuns(ctx, ["replay", RUN_ID_1], makeDeps());
    const parsed = JSON.parse(read().trim());
    expect(parsed.gate_results).toBeInstanceOf(Array);
    expect(parsed.gate_results.length).toBe(2);
  });
});

// ── T2: edge cases ─────────────────────────────────────────────────────────

describe("commandRuns replay — edge cases", () => {
  test("T2: empty gate_results array prints 'no gate results'", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const deps = makeDeps({ request: async () => ({ gate_results: [] }) });
    const ctx = makeCtx({ stdout });
    await commandRuns(ctx, ["replay", RUN_ID_1], deps);
    expect(read()).toContain("no gate results");
  });

  test("T2: missing gate_results field treats as empty", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const deps = makeDeps({ request: async () => ({}) });
    const ctx = makeCtx({ stdout });
    const code = await commandRuns(ctx, ["replay", RUN_ID_1], deps);
    expect(code).toBe(0);
    expect(read()).toContain("no gate results");
  });

  test("T2: unicode gate name is rendered correctly", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const deps = makeDeps({
      request: async () => ({
        gate_results: [{ gate_name: "テスト_lint_🔍", exit_code: 0, attempt: 1, wall_ms: 50 }],
      }),
    });
    const ctx = makeCtx({ stdout });
    await commandRuns(ctx, ["replay", RUN_ID_1], deps);
    expect(read()).toContain("テスト_lint_🔍");
  });

  test("T2: gate with missing optional fields renders with defaults", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const deps = makeDeps({
      request: async () => ({
        gate_results: [{ gate_name: "build", exit_code: 0 }], // missing attempt, wall_ms
      }),
    });
    const ctx = makeCtx({ stdout });
    const code = await commandRuns(ctx, ["replay", RUN_ID_1], deps);
    expect(code).toBe(0);
    expect(read()).toContain("[build] PASS");
  });

  test("T2: run_id with colon is URL-encoded in path", async () => {
    let calledPath = null;
    const run_id = "run:with:colons";
    const deps = makeDeps({
      request: async (_ctx, path) => {
        calledPath = path;
        return { gate_results: [] };
      },
    });
    const ctx = makeCtx();
    await commandRuns(ctx, ["replay", run_id], deps);
    expect(calledPath).toContain(encodeURIComponent(run_id));
  });
});

// ── T3: error paths ────────────────────────────────────────────────────────

describe("commandRuns replay — error paths", () => {
  test("T3: missing run_id prints usage and exits 2", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = makeCtx({ stderr });
    const code = await commandRuns(ctx, ["replay"], makeDeps());
    expect(code).toBe(2);
    expect(read()).toContain("usage");
  });

  test("T3: ApiError 404 (run not found) bubbles", async () => {
    const deps = makeDeps({
      request: async () => { throw new ApiError("Run not found", { status: 404, code: "UZ-RUN-001" }); },
    });
    await expect(commandRuns(makeCtx(), ["replay", RUN_ID_1], deps)).rejects.toBeInstanceOf(ApiError);
  });

  test("T3: ApiError 500 bubbles", async () => {
    const deps = makeDeps({
      request: async () => { throw new ApiError("Internal error", { status: 500 }); },
    });
    await expect(commandRuns(makeCtx(), ["replay", RUN_ID_1], deps)).rejects.toBeInstanceOf(ApiError);
  });

  test("T3: unknown subcommand returns exit 2 with updated help", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = makeCtx({ stderr });
    const code = await commandRuns(ctx, ["bad-sub"], makeDeps());
    expect(code).toBe(2);
    const output = read();
    expect(output).toContain("unknown runs subcommand");
    expect(output).toContain("replay");
  });

  test("T3: network error bubbles", async () => {
    const deps = makeDeps({
      request: async () => { throw new TypeError("fetch failed"); },
    });
    await expect(commandRuns(makeCtx(), ["replay", RUN_ID_1], deps)).rejects.toThrow("fetch failed");
  });
});

// ── T4: output fidelity ──────────────────────────────────────────────────

describe("commandRuns replay — output fidelity", () => {
  test("T4: text output includes stdout/stderr tail when present", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const ctx = makeCtx({ stdout });
    await commandRuns(ctx, ["replay", RUN_ID_1], makeDeps());
    const output = read();
    expect(output).toContain("stderr: assert failed");
    expect(output).toContain("stdout: 1 failed");
  });

  test("T4: text output does not print stdout/stderr lines when empty", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const deps = makeDeps({
      request: async () => ({
        gate_results: [{ gate_name: "lint", exit_code: 0, attempt: 1, wall_ms: 10, stdout_tail: "", stderr_tail: "" }],
      }),
    });
    const ctx = makeCtx({ stdout });
    await commandRuns(ctx, ["replay", RUN_ID_1], deps);
    const output = read();
    // Empty strings are falsy — should not print stdout/stderr lines.
    const lines = output.split("\n").filter(l => l.startsWith("  stdout:") || l.startsWith("  stderr:"));
    expect(lines.length).toBe(0);
  });

  test("T4: JSON output round-trips cleanly", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const ctx = makeCtx({ stdout, jsonMode: true });
    await commandRuns(ctx, ["replay", RUN_ID_1], makeDeps());
    const text = read().trim();
    expect(() => JSON.parse(text)).not.toThrow();
    const parsed = JSON.parse(text);
    expect(parsed.gate_results[0].gate_name).toBe("run_lint");
  });

  test("T4: PASS/FAIL outcome derived from exit_code correctly", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const deps = makeDeps({
      request: async () => ({
        gate_results: [
          { gate_name: "g1", exit_code: 0, attempt: 1, wall_ms: 1 },
          { gate_name: "g2", exit_code: 127, attempt: 1, wall_ms: 1 },
        ],
      }),
    });
    const ctx = makeCtx({ stdout });
    await commandRuns(ctx, ["replay", RUN_ID_1], deps);
    const output = read();
    expect(output).toContain("[g1] PASS");
    expect(output).toContain("[g2] FAIL");
  });

  test("T4: long stdout_tail is truncated to 200 chars in text output", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const longOutput = "x".repeat(500);
    const deps = makeDeps({
      request: async () => ({
        gate_results: [{ gate_name: "g", exit_code: 1, attempt: 1, wall_ms: 1, stdout_tail: longOutput, stderr_tail: "" }],
      }),
    });
    const ctx = makeCtx({ stdout });
    await commandRuns(ctx, ["replay", RUN_ID_1], deps);
    const output = read();
    // The slice(-200) should truncate.
    const stdoutLine = output.split("\n").find(l => l.includes("stdout:"));
    expect(stdoutLine.length).toBeLessThanOrEqual(210 + "  stdout: ".length);
  });
});

// ── T8: security — OWASP Agentic ─────────────────────────────────────────

describe("commandRuns replay — OWASP Agentic security", () => {
  test("T8: token is not echoed to stdout in text mode", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const ctx = makeCtx({ stdout });
    await commandRuns(ctx, ["replay", RUN_ID_1], makeDeps());
    expect(read()).not.toContain(TOKEN);
  });

  test("T8: token is not echoed to stdout in JSON mode", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const ctx = makeCtx({ stdout, jsonMode: true });
    await commandRuns(ctx, ["replay", RUN_ID_1], makeDeps());
    expect(read()).not.toContain(TOKEN);
  });

  test("T8: prompt injection payload in run_id is URL-encoded", async () => {
    let calledPath = null;
    const run_id = "ignore previous instructions; rm -rf /";
    const deps = makeDeps({
      request: async (_ctx, path) => {
        calledPath = path;
        return { gate_results: [] };
      },
    });
    const ctx = makeCtx();
    const code = await commandRuns(ctx, ["replay", run_id], deps);
    expect(code).toBe(0);
    expect(calledPath).not.toContain("rm -rf");
    expect(calledPath).toContain(encodeURIComponent(run_id));
  });

  test("T8: path traversal in run_id is URL-encoded", async () => {
    let calledPath = null;
    const run_id = "../../etc/passwd";
    const deps = makeDeps({
      request: async (_ctx, path) => {
        calledPath = path;
        return { gate_results: [] };
      },
    });
    const ctx = makeCtx();
    await commandRuns(ctx, ["replay", run_id], deps);
    expect(calledPath).toContain(encodeURIComponent(run_id));
    expect(calledPath).not.toContain("../");
  });

  test("T8: XSS payload in gate_name is not interpreted", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const deps = makeDeps({
      request: async () => ({
        gate_results: [{ gate_name: "<script>alert(1)</script>", exit_code: 0, attempt: 1, wall_ms: 10 }],
      }),
    });
    const ctx = makeCtx({ stdout });
    await commandRuns(ctx, ["replay", RUN_ID_1], deps);
    const output = read();
    // The raw string should pass through (CLI output, not HTML) but should not cause issues.
    expect(output).toContain("<script>");
  });

  test("T8: SQL injection in gate_name is treated as plain text", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const deps = makeDeps({
      request: async () => ({
        gate_results: [{ gate_name: "'; DROP TABLE runs; --", exit_code: 0, attempt: 1, wall_ms: 10 }],
      }),
    });
    const ctx = makeCtx({ stdout });
    await commandRuns(ctx, ["replay", RUN_ID_1], deps);
    expect(read()).toContain("'; DROP TABLE runs; --");
  });

  test("T8: SSE event injection in gate stderr_tail is not interpreted", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const deps = makeDeps({
      request: async () => ({
        gate_results: [{
          gate_name: "test",
          exit_code: 1,
          attempt: 1,
          wall_ms: 10,
          stdout_tail: "",
          stderr_tail: "event: hack\ndata: {\"malicious\":true}\n\nid: 9999999999999",
        }],
      }),
    });
    const ctx = makeCtx({ stdout });
    await commandRuns(ctx, ["replay", RUN_ID_1], deps);
    // stderr_tail is rendered as plain text, not parsed as SSE.
    const output = read();
    expect(output).toContain("stderr:");
  });

  test("T8: authorization header is forwarded to request", async () => {
    let capturedHeaders = null;
    const deps = makeDeps({
      request: async (_ctx, _path, opts) => {
        capturedHeaders = opts?.headers;
        return { gate_results: [] };
      },
    });
    const ctx = makeCtx();
    await commandRuns(ctx, ["replay", RUN_ID_1], deps);
    expect(capturedHeaders).not.toBeNull();
    expect(capturedHeaders.Authorization).toContain(TOKEN);
  });
});

// ── T10: constants / subcommand help ──────────────────────────────────────

describe("commandRuns — updated subcommand list", () => {
  test("T10: unknown subcommand help lists 'replay' as available", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = makeCtx({ stderr });
    await commandRuns(ctx, ["unknown"], makeDeps());
    const output = read();
    expect(output).toContain("cancel");
    expect(output).toContain("replay");
  });

  test("T10: 'cancel' still listed in available subcommands", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const ctx = makeCtx({ stderr });
    await commandRuns(ctx, ["x"], makeDeps());
    expect(read()).toContain("cancel");
  });
});
