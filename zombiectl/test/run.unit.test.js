import { describe, test, expect } from "bun:test";
import { makeNoop, makeBufferStream, ui, WS_ID, RUN_ID_1 } from "./helpers.js";
import { createCoreHandlers } from "../src/commands/core.js";

function makeDeps(overrides = {}) {
  return {
    clearCredentials: async () => {},
    createSpinner: () => ({ start() {}, succeed() {}, fail() {} }),
    newIdempotencyKey: () => "idem_test",
    openUrl: async () => false,
    parseFlags: (tokens) => {
      const options = {};
      const positionals = [];
      for (let i = 0; i < tokens.length; i++) {
        if (tokens[i].startsWith("--")) {
          const key = tokens[i].slice(2);
          const next = tokens[i + 1];
          if (next && !next.startsWith("--")) { options[key] = next; i++; }
          else options[key] = true;
        } else { positionals.push(tokens[i]); }
      }
      return { options, positionals };
    },
    printJson: (_s, v) => {},
    printKeyValue: (stream, rows) => {
      for (const [key, value] of Object.entries(rows)) stream.write(`${key}: ${value}\n`);
    },
    printSection: (stream, title) => stream.write(`${title}\n`),
    printTable: () => {},
    request: async () => ({}),
    saveCredentials: async () => {},
    saveWorkspaces: async () => {},
    ui,
    writeLine: (stream, line = "") => stream.write(`${line}\n`),
    apiHeaders: () => ({}),
    ...overrides,
  };
}

describe("commandRun", () => {
  test("successful run creation", async () => {
    const out = makeBufferStream();
    let calledPath = null;
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        calledPath = reqPath;
        if (reqPath.includes("/v1/specs")) {
          return { data: [{ spec_id: "spec_1" }], has_more: false, next_cursor: null };
        }
        return { run_id: RUN_ID_1, state: "SPEC_QUEUED", attempt: 1, plan_tier: "free", credit_remaining_cents: 958, credit_currency: "USD" };
      },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandRun(["--workspace-id", WS_ID]);
    expect(code).toBe(0);
    expect(out.read()).toContain("Run queued");
    expect(out.read()).toContain(RUN_ID_1);
    expect(out.read()).toContain("credit_remaining_cents: 958");
  });

  test("auto-picks first spec when no --spec-id", async () => {
    let runPayload = null;
    const deps = makeDeps({
      request: async (_ctx, reqPath, opts) => {
        if (reqPath.includes("/v1/specs")) {
          return { data: [{ spec_id: "auto_spec" }], has_more: false, next_cursor: null };
        }
        if (reqPath === "/v1/runs") {
          runPayload = JSON.parse(opts.body);
          return { run_id: RUN_ID_1, state: "SPEC_QUEUED", attempt: 1 };
        }
        return {};
      },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    await core.commandRun([]);
    expect(runPayload.spec_id).toBe("auto_spec");
  });

  test("missing workspace error", async () => {
    const err = makeBufferStream();
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandRun([]);
    expect(code).toBe(2);
    expect(err.read()).toContain("workspace_id required");
  });

  test("run status subcommand", async () => {
    const out = makeBufferStream();
    let calledPath = null;
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        calledPath = reqPath;
        return {
          run_id: RUN_ID_1,
          state: "COMPLETED",
          attempt: 1,
          run_snapshot_version: "pver_1",
        };
      },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandRun(["status", RUN_ID_1]);
    expect(code).toBe(0);
    expect(calledPath).toContain(RUN_ID_1);
    expect(out.read()).toContain("Run status");
    expect(out.read()).toContain("COMPLETED");
  });

  test("run status without run_id returns error", async () => {
    const err = makeBufferStream();
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandRun(["status"]);
    expect(code).toBe(2);
    expect(err.read()).toContain("run status requires <run_id>");
  });

  test("successful run output includes plan_tier and credit_currency", async () => {
    const out = makeBufferStream();
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        if (reqPath.includes("/v1/specs")) {
          return { data: [{ spec_id: "spec_1" }], has_more: false, next_cursor: null };
        }
        return { run_id: RUN_ID_1, state: "SPEC_QUEUED", attempt: 1, plan_tier: "scale", credit_remaining_cents: 5000, credit_currency: "EUR" };
      },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandRun(["--workspace-id", WS_ID]);
    expect(code).toBe(0);
    const output = out.read();
    expect(output).toContain("plan_tier: scale");
    expect(output).toContain("credit_currency: EUR");
  });
});
