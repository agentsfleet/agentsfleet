import { describe, test, expect } from "bun:test";
import { makeNoop, makeBufferStream, ui, WS_ID } from "./helpers.js";
import { createCoreHandlers } from "../src/commands/core.js";

const WS_ID_2 = "0195b4ba-8d3a-7f13-8abc-000000000099";

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

describe("commandWorkspace", () => {
  test("list shows table", async () => {
    let tableRows = null;
    const deps = makeDeps({
      printTable: (_s, _cols, rows) => { tableRows = rows; },
    });
    const items = [
      { workspace_id: WS_ID },
      { workspace_id: WS_ID_2 },
    ];
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["list"]);
    expect(code).toBe(0);
    expect(tableRows.length).toBe(2);
    expect(tableRows[0].active).toBe("*");
    expect(tableRows[1].active).toBe("");
  });

  test("list empty state", async () => {
    const out = makeBufferStream();
    const deps = makeDeps({
      printTable: () => {},
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["list"]);
    expect(code).toBe(0);
    expect(out.read()).toContain("no workspaces");
  });

  test("delete by ID", async () => {
    let savedWs = null;
    const deps = makeDeps({
      saveWorkspaces: async (ws) => { savedWs = ws; },
    });
    const items = [{ workspace_id: WS_ID }];
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [...items] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["delete", WS_ID]);
    expect(code).toBe(0);
    expect(savedWs.items.length).toBe(0);
  });

  test("delete updates current workspace", async () => {
    let savedWs = null;
    const deps = makeDeps({
      saveWorkspaces: async (ws) => { savedWs = ws; },
    });
    const items = [
      { workspace_id: WS_ID },
      { workspace_id: WS_ID_2 },
    ];
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [...items] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["delete", WS_ID]);
    expect(code).toBe(0);
    expect(savedWs.current_workspace_id).toBe(WS_ID_2);
  });

  test("delete without id returns error", async () => {
    const err = makeBufferStream();
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["delete"]);
    expect(code).toBe(2);
    expect(err.read()).toContain("workspace delete requires");
  });

});
