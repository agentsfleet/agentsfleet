import { describe, test, expect } from "bun:test";
import { makeNoop, makeBufferStream, ui, WS_ID } from "./helpers.js";
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

describe("commandSpecsSync", () => {
  test("successful sync with workspace", async () => {
    const out = makeBufferStream();
    let calledPath = null;
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        calledPath = reqPath;
        return { synced_count: 3, total_pending: 0 };
      },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandSpecsSync([]);
    expect(code).toBe(0);
    expect(calledPath).toContain(WS_ID);
    expect(out.read()).toContain("Specs synced");
    expect(out.read()).toContain(WS_ID);
  });

  test("missing workspace_id error", async () => {
    const err = makeBufferStream();
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandSpecsSync([]);
    expect(code).toBe(2);
    expect(err.read()).toContain("workspace_id required");
  });

  test("JSON mode output", async () => {
    let printed = null;
    const deps = makeDeps({
      request: async () => ({ synced_count: 5, total_pending: 2 }),
      printJson: (_s, v) => { printed = v; },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: true, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandSpecsSync([]);
    expect(code).toBe(0);
    expect(printed.synced_count).toBe(5);
  });
});
