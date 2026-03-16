import { describe, test, expect } from "bun:test";
import { makeNoop, makeBufferStream, ui, WS_ID, RUN_ID_1, RUN_ID_2 } from "./helpers.js";
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

const SAMPLE_RUNS = [
  { run_id: RUN_ID_1, workspace_id: WS_ID, state: "COMPLETED" },
  { run_id: RUN_ID_2, workspace_id: WS_ID, state: "RUNNING" },
];

describe("commandRunsList", () => {
  test("successful server query", async () => {
    let calledPath = null;
    let tableRows = null;
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        calledPath = reqPath;
        return { runs: SAMPLE_RUNS };
      },
      printTable: (_s, _cols, rows) => { tableRows = rows; },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandRunsList([]);
    expect(code).toBe(0);
    expect(calledPath).toContain("/v1/runs");
    expect(calledPath).toContain(WS_ID);
    expect(tableRows.length).toBe(2);
  });

  test("empty results", async () => {
    const out = makeBufferStream();
    const deps = makeDeps({
      request: async () => ({ runs: [] }),
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandRunsList([]);
    expect(code).toBe(0);
    expect(out.read()).toContain("no runs");
  });

  test("workspace filter passed in URL", async () => {
    let calledPath = null;
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        calledPath = reqPath;
        return { runs: [] };
      },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    await core.commandRunsList(["--workspace-id", WS_ID]);
    expect(calledPath).toContain(`workspace_id=${encodeURIComponent(WS_ID)}`);
  });

  test("JSON mode", async () => {
    let printed = null;
    const deps = makeDeps({
      request: async () => ({ runs: SAMPLE_RUNS }),
      printJson: (_s, v) => { printed = v; },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: true, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandRunsList([]);
    expect(code).toBe(0);
    expect(printed.runs.length).toBe(2);
    expect(printed.total).toBe(2);
  });
});
