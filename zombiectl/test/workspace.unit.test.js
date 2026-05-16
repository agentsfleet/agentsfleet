import { describe, test, expect } from "bun:test";
import {
  createCoreHandlers,
  makeBufferStream,
  makeNoop,
  ui,
  WS_ID,
} from "./helpers.ts";
const WS_ID_2 = "0195b4ba-8d3a-7f13-8abc-000000000099";

function makeDeps(overrides = {}) {
  return {
    clearCredentials: async () => {},
    createSpinner: () => ({ start() {}, succeed() {}, fail() {} }),
    newIdempotencyKey: () => "idem_test",
    openUrl: async () => false,
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

  test("list jsonMode prints structured payload", async () => {
    let printed = null;
    const deps = makeDeps({
      printJson: (_s, v) => { printed = v; },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: true, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [{ workspace_id: WS_ID }] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["list"]);
    expect(code).toBe(0);
    expect(printed.current_workspace_id).toBe(WS_ID);
    expect(printed.workspaces).toHaveLength(1);
  });

  test("use without id reports USAGE_ERROR", async () => {
    const err = makeBufferStream();
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["use"]);
    expect(code).toBe(2);
    expect(err.read()).toContain("workspace use requires");
  });

  test("use with malformed id reports VALIDATION_ERROR", async () => {
    const err = makeBufferStream();
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["use", "not-a-uuid"]);
    expect(code).toBe(2);
    // ValidateRequiredId returns "must be a uuid v7"-style message.
    const msg = err.read();
    expect(msg.length).toBeGreaterThan(0);
  });

  test("use with unknown valid id reports UNKNOWN_WORKSPACE", async () => {
    const err = makeBufferStream();
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["use", WS_ID]);
    expect(code).toBe(2);
    expect(err.read()).toContain("not in your local list");
  });

  test("use with known id activates workspace and persists", async () => {
    let saved = null;
    const deps = makeDeps({
      saveWorkspaces: async (ws) => { saved = ws; },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [{ workspace_id: WS_ID, name: "main" }] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["use", WS_ID]);
    expect(code).toBe(0);
    expect(saved.current_workspace_id).toBe(WS_ID);
  });

  test("show without active workspace reports NO_WORKSPACE", async () => {
    const err = makeBufferStream();
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["show"]);
    expect(code).toBe(2);
    expect(err.read()).toContain("no active workspace");
  });

  test("show jsonMode renders detail payload for active workspace", async () => {
    let printed;
    const deps = makeDeps({
      printJson: (_s, v) => { printed = v; },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: true, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [{ workspace_id: WS_ID, name: "main" }] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["show"]);
    expect(code).toBe(0);
    expect(printed.workspace_id).toBe(WS_ID);
    expect(printed.active).toBe(true);
  });

  test("credentials redirect action returns 0", async () => {
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["credentials"]);
    expect(code).toBe(0);
  });

  test("credentials jsonMode emits the redirect payload", async () => {
    let printed;
    const deps = makeDeps({
      printJson: (_s, v) => { printed = v; },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: true, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["credentials"]);
    expect(code).toBe(0);
    expect(printed?.status).toBe("redirect");
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
