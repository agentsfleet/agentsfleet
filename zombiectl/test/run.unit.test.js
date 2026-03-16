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
    printKeyValue: () => {},
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
          return { specs: [{ spec_id: "spec_1" }] };
        }
        return { run_id: RUN_ID_1, state: "SPEC_QUEUED", attempt: 1 };
      },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandRun(["--workspace-id", WS_ID]);
    expect(code).toBe(0);
    expect(out.read()).toContain("run queued");
  });

  test("auto-picks first spec when no --spec-id", async () => {
    let runPayload = null;
    const deps = makeDeps({
      request: async (_ctx, reqPath, opts) => {
        if (reqPath.includes("/v1/specs")) {
          return { specs: [{ spec_id: "auto_spec" }] };
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
});
