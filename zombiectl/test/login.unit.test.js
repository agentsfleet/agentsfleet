import { describe, test, expect } from "bun:test";
import { makeNoop, makeBufferStream, ui } from "./helpers.js";
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
        return { status: "complete", token: "tok_123" };
      },
      saveCredentials: async (creds) => {
        expect(creds.token).toBe("tok_123");
      },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, noOpen: true, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandLogin([]);
    expect(code).toBe(0);
    expect(out.read()).toContain("login complete");
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
