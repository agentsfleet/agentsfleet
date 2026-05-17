import { describe, test, expect } from "bun:test";
import {
  createCoreHandlers,
  makeBufferStream,
  makeNoop,
  ui,
} from "./helpers.ts";
import type {
  CommandCtx,
  CommandDeps,
  Credentials,
  Workspaces,
} from "../src/commands/types.ts";

const TENANT_WORKSPACES_PATH = "/v1/tenants/me/workspaces";
const LOGIN_TOKEN = "tok_123";
const DEFAULT_WORKSPACE_ID = "ws_signup_default";
const DEFAULT_WORKSPACE_NAME = "jolly-harbor-482";

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
    noOpen: true,
    apiUrl: "https://api.test",
    env: {},
    ...over,
  };
}

describe("commandLogin", () => {
  test("successful login flow", async () => {
    const out = makeBufferStream();
    let _pollCount = 0;
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        if (reqPath === "/v1/auth/sessions") {
          return { session_id: "sess_1", login_url: "https://login.test" };
        }
        _pollCount++;
        return { status: "complete", token: LOGIN_TOKEN };
      },
      saveCredentials: async (creds) => {
        expect(creds.token).toBe(LOGIN_TOKEN);
      },
    });
    const ctx = makeCtx({ stdout: out.stream });
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandLogin([]);
    expect(code).toBe(0);
    expect(out.read()).toContain("login complete");
  });

  test("successful login selects the signup-created default workspace", async () => {
    const out = makeBufferStream();
    const seenPaths: string[] = [];
    // Boxed capture — see D42c-net Discovery: bare `let` assigned inside an async
    // callback narrows to `never` after await, defeating later property reads.
    const captured: { ws: Workspaces | null } = { ws: null };
    const deps = makeDeps({
      request: async (_ctx, reqPath, options) => {
        seenPaths.push(reqPath);
        if (reqPath === "/v1/auth/sessions") {
          return { session_id: "sess_workspace", login_url: "https://login.test" };
        }
        if (reqPath === "/v1/auth/sessions/sess_workspace") {
          return { status: "complete", token: LOGIN_TOKEN };
        }
        if (reqPath === TENANT_WORKSPACES_PATH) {
          expect(options?.headers?.["authorization"]).toBe(`Bearer ${LOGIN_TOKEN}`);
          return {
            items: [{ id: DEFAULT_WORKSPACE_ID, name: DEFAULT_WORKSPACE_NAME, created_at: 1234 }],
          };
        }
        throw new Error(`unexpected path: ${reqPath}`);
      },
      saveCredentials: async () => {},
      saveWorkspaces: async (workspaces) => { captured.ws = workspaces; },
      apiHeaders: (ctx) => ({ authorization: `Bearer ${ctx.token ?? ""}` }),
    });
    const ctx = makeCtx({ stdout: out.stream });
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);

    const code = await core.commandLogin([]);

    expect(code).toBe(0);
    expect(seenPaths).toContain(TENANT_WORKSPACES_PATH);
    const saved = captured.ws;
    if (!saved) throw new Error("expected saveWorkspaces to be called");
    expect(saved.current_workspace_id).toBe(DEFAULT_WORKSPACE_ID);
    expect(saved.items).toEqual([
      { workspace_id: DEFAULT_WORKSPACE_ID, name: DEFAULT_WORKSPACE_NAME, created_at: 1234 },
    ]);
  });

  test("hydration with an empty items[] response does not write workspaces.json", async () => {
    let saveCalled = false;
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        if (reqPath === "/v1/auth/sessions") return { session_id: "sess_empty", login_url: "https://login.test" };
        if (reqPath === "/v1/auth/sessions/sess_empty") return { status: "complete", token: LOGIN_TOKEN };
        if (reqPath === TENANT_WORKSPACES_PATH) return { items: [], total: 0 };
        throw new Error(`unexpected path: ${reqPath}`);
      },
      saveCredentials: async () => {},
      saveWorkspaces: async () => { saveCalled = true; },
    });
    const ctx = makeCtx();
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);

    const code = await core.commandLogin([]);
    expect(code).toBe(0);
    expect(saveCalled).toBe(false);
    expect(workspaces.current_workspace_id).toBeNull();
    expect(workspaces.items).toEqual([]);
  });

  test("hydration preserves a pre-existing current_workspace_id when the server returns it", async () => {
    const captured: { ws: Workspaces | null } = { ws: null };
    const items = [
      { id: "ws_one", name: "one", created_at: 100 },
      { id: "ws_two", name: "two", created_at: 200 },
    ];
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        if (reqPath === "/v1/auth/sessions") return { session_id: "sess_multi", login_url: "https://login.test" };
        if (reqPath === "/v1/auth/sessions/sess_multi") return { status: "complete", token: LOGIN_TOKEN };
        if (reqPath === TENANT_WORKSPACES_PATH) return { items, total: 2 };
        throw new Error(`unexpected path: ${reqPath}`);
      },
      saveCredentials: async () => {},
      saveWorkspaces: async (next) => { captured.ws = next; },
    });
    const ctx = makeCtx();
    // Pre-existing local selection points at ws_two; hydration must keep it.
    const workspaces: Workspaces = { current_workspace_id: "ws_two", items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);

    const code = await core.commandLogin([]);
    expect(code).toBe(0);
    const saved = captured.ws;
    if (!saved) throw new Error("expected saveWorkspaces to be called");
    expect(saved.current_workspace_id).toBe("ws_two");
    expect(saved.items.map((w) => w.workspace_id)).toEqual(["ws_one", "ws_two"]);
  });

  test("successful login keeps credentials when workspace hydration fails", async () => {
    const captured: { token: string | null } = { token: null };
    let savedWorkspaces = false;
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        if (reqPath === "/v1/auth/sessions") {
          return { session_id: "sess_workspace_down", login_url: "https://login.test" };
        }
        if (reqPath === "/v1/auth/sessions/sess_workspace_down") {
          return { status: "complete", token: LOGIN_TOKEN };
        }
        if (reqPath === TENANT_WORKSPACES_PATH) throw new Error("workspace list unavailable");
        throw new Error(`unexpected path: ${reqPath}`);
      },
      saveCredentials: async (creds: Credentials) => { captured.token = creds.token; },
      saveWorkspaces: async () => { savedWorkspaces = true; },
    });
    const ctx = makeCtx();
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);

    const code = await core.commandLogin([]);

    expect(code).toBe(0);
    expect(captured.token).toBe(LOGIN_TOKEN);
    expect(savedWorkspaces).toBe(false);
    expect(workspaces.current_workspace_id).toBeNull();
  });

  test("successful login exits 0 when saveWorkspaces throws (disk full / permissions)", async () => {
    const captured: { token: string | null } = { token: null };
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        if (reqPath === "/v1/auth/sessions") {
          return { session_id: "sess_disk_full", login_url: "https://login.test" };
        }
        if (reqPath === "/v1/auth/sessions/sess_disk_full") {
          return { status: "complete", token: LOGIN_TOKEN };
        }
        if (reqPath === TENANT_WORKSPACES_PATH) {
          return {
            items: [
              { workspace_id: "01HXXXXXXXXXXXXXXXXXXXXXXX", name: "default", created_at: 1_700_000_000_000 },
            ],
          };
        }
        throw new Error(`unexpected path: ${reqPath}`);
      },
      saveCredentials: async (creds: Credentials) => { captured.token = creds.token; },
      saveWorkspaces: async () => { throw new Error("ENOSPC: no space left on device"); },
    });
    const ctx = makeCtx();
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);

    const code = await core.commandLogin([]);

    expect(code).toBe(0);
    expect(captured.token).toBe(LOGIN_TOKEN);
  });

  test("expired session returns 1", async () => {
    const err = makeBufferStream();
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        if (reqPath === "/v1/auth/sessions") {
          return { session_id: "sess_2", login_url: "https://login.test" };
        }
        return { status: "expired" };
      },
    });
    const ctx = makeCtx({ stderr: err.stream });
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandLogin([]);
    expect(code).toBe(1);
    expect(err.read()).toContain("expired");
  });

  test("timeout returns 1", async () => {
    const err = makeBufferStream();
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        if (reqPath === "/v1/auth/sessions") {
          return { session_id: "sess_3", login_url: "https://login.test" };
        }
        return { status: "pending", token: null };
      },
    });
    const ctx = makeCtx({ stderr: err.stream });
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    // use very short timeout
    const code = await core.commandLogin(["--timeout-sec", "1", "--poll-ms", "100"]);
    expect(code).toBe(1);
    expect(err.read()).toContain("timed out");
  });

  test("--no-open flag skips browser", async () => {
    let browserOpened = false;
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        if (reqPath === "/v1/auth/sessions") {
          return { session_id: "sess_4", login_url: "https://login.test" };
        }
        return { status: "complete", token: "tok_456" };
      },
      openUrl: async () => { browserOpened = true; return true; },
    });
    const ctx = makeCtx({ noOpen: false });
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    await core.commandLogin(["--no-open"]);
    expect(browserOpened).toBe(false);
  });
});
