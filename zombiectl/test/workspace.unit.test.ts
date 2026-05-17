import { describe, test, expect } from "bun:test";
import {
  createCoreHandlers,
  makeBufferStream,
  makeNoop,
  ui,
  WS_ID,
} from "./helpers.ts";
import type {
  CommandCtx,
  CommandDeps,
  WorkspaceItem,
  Workspaces,
} from "../src/commands/types.ts";

const WS_ID_2 = "0195b4ba-8d3a-7f13-8abc-000000000099";

function makeDeps(overrides: Partial<CommandDeps> = {}): CommandDeps {
  const base = {
    clearCredentials: async () => {},
    createSpinner: () => ({ start() {}, stop() {}, succeed() {}, fail() {} }),
    newIdempotencyKey: () => "idem_test",
    openUrl: async () => false,
    printJson: (_s: NodeJS.WritableStream, _v: unknown) => {},
    printKeyValue: () => {},
    printSection: () => {},
    printTable: () => {},
    request: async () => ({}),
    saveCredentials: async () => {},
    saveWorkspaces: async () => {},
    ui,
    writeLine: (stream: NodeJS.WritableStream, line = "") => stream.write(`${line}\n`),
    apiHeaders: () => ({}),
    ...overrides,
  };
  return base as unknown as CommandDeps;
}

function makeCtx(over: Partial<CommandCtx> = {}): CommandCtx {
  return {
    stdout: makeNoop(),
    stderr: makeNoop(),
    jsonMode: false,
    apiUrl: "https://api.test",
    env: {},
    ...over,
  };
}

// printTable rows are richer than the bare WorkspaceItem (carry an `active`
// marker and stringified ID) — captured loosely so the test can read both
// the original fields and the dispatcher-projected `active` cell.
interface TableRow {
  active: string;
  workspace_id: string;
  [key: string]: unknown;
}

interface WorkspaceListJson {
  current_workspace_id: string | null;
  workspaces: WorkspaceItem[];
}

interface WorkspaceShowJson {
  workspace_id: string;
  active: boolean;
  [key: string]: unknown;
}

interface RedirectJson {
  status: string;
}

function mkItem(id: string, name = "ws", created = 0): WorkspaceItem {
  return { workspace_id: id, name, created_at: created };
}

describe("commandWorkspace", () => {
  test("list shows table", async () => {
    const captured: { rows: TableRow[] | null } = { rows: null };
    const deps = makeDeps({
      printTable: (_s, _cols, rows) => { captured.rows = rows as TableRow[]; },
    });
    const items = [mkItem(WS_ID), mkItem(WS_ID_2)];
    const workspaces: Workspaces = { current_workspace_id: WS_ID, items };
    const core = createCoreHandlers(makeCtx(), workspaces, deps);
    const code = await core.commandWorkspace(["list"]);
    expect(code).toBe(0);
    expect(captured.rows).not.toBeNull();
    const rows = captured.rows ?? [];
    expect(rows.length).toBe(2);
    expect(rows[0]?.active).toBe("*");
    expect(rows[1]?.active).toBe("");
  });

  test("list empty state", async () => {
    const out = makeBufferStream();
    const deps = makeDeps({
      printTable: () => {},
    });
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(makeCtx({ stdout: out.stream }), workspaces, deps);
    const code = await core.commandWorkspace(["list"]);
    expect(code).toBe(0);
    expect(out.read()).toContain("no workspaces");
  });

  test("delete by ID", async () => {
    const captured: { ws: Workspaces | null } = { ws: null };
    const deps = makeDeps({
      saveWorkspaces: async (ws) => { captured.ws = ws; },
    });
    const workspaces: Workspaces = { current_workspace_id: WS_ID, items: [mkItem(WS_ID)] };
    const core = createCoreHandlers(makeCtx(), workspaces, deps);
    const code = await core.commandWorkspace(["delete", WS_ID]);
    expect(code).toBe(0);
    const saved = captured.ws;
    if (!saved) throw new Error("expected saveWorkspaces to be called");
    expect(saved.items.length).toBe(0);
  });

  test("delete updates current workspace", async () => {
    const captured: { ws: Workspaces | null } = { ws: null };
    const deps = makeDeps({
      saveWorkspaces: async (ws) => { captured.ws = ws; },
    });
    const workspaces: Workspaces = {
      current_workspace_id: WS_ID,
      items: [mkItem(WS_ID), mkItem(WS_ID_2)],
    };
    const core = createCoreHandlers(makeCtx(), workspaces, deps);
    const code = await core.commandWorkspace(["delete", WS_ID]);
    expect(code).toBe(0);
    const saved = captured.ws;
    if (!saved) throw new Error("expected saveWorkspaces to be called");
    expect(saved.current_workspace_id).toBe(WS_ID_2);
  });

  test("list jsonMode prints structured payload", async () => {
    const captured: { json: WorkspaceListJson | null } = { json: null };
    const deps = makeDeps({
      printJson: (_s, v) => { captured.json = v as WorkspaceListJson; },
    });
    const workspaces: Workspaces = { current_workspace_id: WS_ID, items: [mkItem(WS_ID)] };
    const core = createCoreHandlers(makeCtx({ jsonMode: true }), workspaces, deps);
    const code = await core.commandWorkspace(["list"]);
    expect(code).toBe(0);
    expect(captured.json?.current_workspace_id).toBe(WS_ID);
    expect(captured.json?.workspaces).toHaveLength(1);
  });

  test("use without id reports USAGE_ERROR", async () => {
    const err = makeBufferStream();
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(makeCtx({ stderr: err.stream }), workspaces, makeDeps());
    const code = await core.commandWorkspace(["use"]);
    expect(code).toBe(2);
    expect(err.read()).toContain("workspace use requires");
  });

  test("use with malformed id reports VALIDATION_ERROR", async () => {
    const err = makeBufferStream();
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(makeCtx({ stderr: err.stream }), workspaces, makeDeps());
    const code = await core.commandWorkspace(["use", "not-a-uuid"]);
    expect(code).toBe(2);
    // ValidateRequiredId returns "must be a uuid v7"-style message.
    const msg = err.read();
    expect(msg.length).toBeGreaterThan(0);
  });

  test("use with unknown valid id reports UNKNOWN_WORKSPACE", async () => {
    const err = makeBufferStream();
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(makeCtx({ stderr: err.stream }), workspaces, makeDeps());
    const code = await core.commandWorkspace(["use", WS_ID]);
    expect(code).toBe(2);
    expect(err.read()).toContain("not in your local list");
  });

  test("use with known id activates workspace and persists", async () => {
    const captured: { ws: Workspaces | null } = { ws: null };
    const deps = makeDeps({
      saveWorkspaces: async (ws) => { captured.ws = ws; },
    });
    const workspaces: Workspaces = {
      current_workspace_id: null,
      items: [mkItem(WS_ID, "main")],
    };
    const core = createCoreHandlers(makeCtx(), workspaces, deps);
    const code = await core.commandWorkspace(["use", WS_ID]);
    expect(code).toBe(0);
    const saved = captured.ws;
    if (!saved) throw new Error("expected saveWorkspaces to be called");
    expect(saved.current_workspace_id).toBe(WS_ID);
  });

  test("show without active workspace reports NO_WORKSPACE", async () => {
    const err = makeBufferStream();
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(makeCtx({ stderr: err.stream }), workspaces, makeDeps());
    const code = await core.commandWorkspace(["show"]);
    expect(code).toBe(2);
    expect(err.read()).toContain("no active workspace");
  });

  test("show jsonMode renders detail payload for active workspace", async () => {
    const captured: { json: WorkspaceShowJson | null } = { json: null };
    const deps = makeDeps({
      printJson: (_s, v) => { captured.json = v as WorkspaceShowJson; },
    });
    const workspaces: Workspaces = {
      current_workspace_id: WS_ID,
      items: [mkItem(WS_ID, "main")],
    };
    const core = createCoreHandlers(makeCtx({ jsonMode: true }), workspaces, deps);
    const code = await core.commandWorkspace(["show"]);
    expect(code).toBe(0);
    expect(captured.json?.workspace_id).toBe(WS_ID);
    expect(captured.json?.active).toBe(true);
  });

  test("credentials redirect action returns 0", async () => {
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(makeCtx(), workspaces, makeDeps());
    const code = await core.commandWorkspace(["credentials"]);
    expect(code).toBe(0);
  });

  test("credentials jsonMode emits the redirect payload", async () => {
    const captured: { json: RedirectJson | null } = { json: null };
    const deps = makeDeps({
      printJson: (_s, v) => { captured.json = v as RedirectJson; },
    });
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(makeCtx({ jsonMode: true }), workspaces, deps);
    const code = await core.commandWorkspace(["credentials"]);
    expect(code).toBe(0);
    expect(captured.json?.status).toBe("redirect");
  });

  test("delete without id returns error", async () => {
    const err = makeBufferStream();
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(makeCtx({ stderr: err.stream }), workspaces, makeDeps());
    const code = await core.commandWorkspace(["delete"]);
    expect(code).toBe(2);
    expect(err.read()).toContain("workspace delete requires");
  });

});
