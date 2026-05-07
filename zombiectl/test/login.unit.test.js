import { describe, test, expect } from "bun:test";
import { makeNoop, makeBufferStream, ui } from "./helpers.js";
import { createCoreHandlers } from "../src/commands/core.js";

const TENANT_WORKSPACES_PATH = "/v1/tenants/me/workspaces";
const LOGIN_TOKEN = "tok_123";
const DEFAULT_WORKSPACE_ID = "ws_signup_default";
const DEFAULT_WORKSPACE_NAME = "jolly-harbor-482";

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

describe("commandLogin", () => {
  test("successful login flow", async () => {
    const out = makeBufferStream();
    let pollCount = 0;
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        if (reqPath === "/v1/auth/sessions") {
          return { session_id: "sess_1", login_url: "https://login.test" };
        }
        pollCount++;
        return { status: "complete", token: LOGIN_TOKEN };
      },
      saveCredentials: async (creds) => {
        expect(creds.token).toBe(LOGIN_TOKEN);
      },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, noOpen: true, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandLogin([]);
    expect(code).toBe(0);
    expect(out.read()).toContain("login complete");
  });

  test("successful login selects the signup-created default workspace", async () => {
    const out = makeBufferStream();
    const seenPaths = [];
    let savedWorkspaces = null;
    const deps = makeDeps({
      request: async (_ctx, reqPath, options = {}) => {
        seenPaths.push(reqPath);
        if (reqPath === "/v1/auth/sessions") {
          return { session_id: "sess_workspace", login_url: "https://login.test" };
        }
        if (reqPath === "/v1/auth/sessions/sess_workspace") {
          return { status: "complete", token: LOGIN_TOKEN };
        }
        if (reqPath === TENANT_WORKSPACES_PATH) {
          expect(options.headers.authorization).toBe(`Bearer ${LOGIN_TOKEN}`);
          return {
            items: [{ id: DEFAULT_WORKSPACE_ID, name: DEFAULT_WORKSPACE_NAME, created_at: 1234 }],
          };
        }
        throw new Error(`unexpected path: ${reqPath}`);
      },
      saveCredentials: async () => {},
      saveWorkspaces: async (workspaces) => { savedWorkspaces = workspaces; },
      apiHeaders: (ctx) => ({ authorization: `Bearer ${ctx.token}` }),
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, noOpen: true, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);

    const code = await core.commandLogin([]);

    expect(code).toBe(0);
    expect(seenPaths).toContain(TENANT_WORKSPACES_PATH);
    expect(savedWorkspaces.current_workspace_id).toBe(DEFAULT_WORKSPACE_ID);
    expect(savedWorkspaces.items).toEqual([
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
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, noOpen: true, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);

    const code = await core.commandLogin([]);
    expect(code).toBe(0);
    expect(saveCalled).toBe(false);
    expect(workspaces.current_workspace_id).toBeNull();
    expect(workspaces.items).toEqual([]);
  });

  test("hydration preserves a pre-existing current_workspace_id when the server returns it", async () => {
    let saved = null;
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
      saveWorkspaces: async (next) => { saved = next; },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, noOpen: true, env: {} };
    // Pre-existing local selection points at ws_two; hydration must keep it.
    const workspaces = { current_workspace_id: "ws_two", items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);

    const code = await core.commandLogin([]);
    expect(code).toBe(0);
    expect(saved.current_workspace_id).toBe("ws_two");
    expect(saved.items.map((w) => w.workspace_id)).toEqual(["ws_one", "ws_two"]);
  });

  test("successful login keeps credentials when workspace hydration fails", async () => {
    let savedToken = null;
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
      saveCredentials: async (creds) => { savedToken = creds.token; },
      saveWorkspaces: async () => { savedWorkspaces = true; },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, noOpen: true, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);

    const code = await core.commandLogin([]);

    expect(code).toBe(0);
    expect(savedToken).toBe(LOGIN_TOKEN);
    expect(savedWorkspaces).toBe(false);
    expect(workspaces.current_workspace_id).toBeNull();
  });

  test("successful login exits 0 when saveWorkspaces throws (disk full / permissions)", async () => {
    let savedToken = null;
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
      saveCredentials: async (creds) => { savedToken = creds.token; },
      saveWorkspaces: async () => { throw new Error("ENOSPC: no space left on device"); },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, noOpen: true, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);

    const code = await core.commandLogin([]);

    expect(code).toBe(0);
    expect(savedToken).toBe(LOGIN_TOKEN);
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
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, noOpen: true, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
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
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, noOpen: true, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
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
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, noOpen: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    await core.commandLogin(["--no-open"]);
    expect(browserOpened).toBe(false);
  });
});
