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

function makeConsentDeps({ promptResponse = true, recordedSave } = {}) {
  return {
    request: async (_ctx, reqPath) => {
      if (reqPath === "/v1/auth/sessions") {
        return { session_id: "sess_consent", login_url: "https://login.test" };
      }
      return { status: "complete", token: "tok_consent" };
    },
    loadPreferences: async () => ({ schema_version: 1, posthog_enabled: null, decided_at: null }),
    savePreferences: async (next) => {
      if (recordedSave) recordedSave.value = next;
    },
    promptYesNo: async () => promptResponse,
  };
}

describe("commandLogin telemetry consent prompt", () => {
  test("prompts on first interactive login and persists yes", async () => {
    const recorded = { value: null };
    let promptCalls = 0;
    const deps = makeDeps({
      ...makeConsentDeps({ promptResponse: true, recordedSave: recorded }),
      promptYesNo: async (_stdin, _stdout, msg) => {
        promptCalls++;
        expect(msg).toContain("anonymous usage metrics");
        return true;
      },
    });
    const ctx = {
      stdout: makeNoop(), stderr: makeNoop(),
      jsonMode: false, noOpen: true, noInput: false,
      env: {},
      preferences: { posthog_enabled: null },
    };
    const core = createCoreHandlers(ctx, { current_workspace_id: null, items: [] }, deps);
    const code = await core.commandLogin([]);
    expect(code).toBe(0);
    expect(promptCalls).toBe(1);
    expect(recorded.value).not.toBeNull();
    expect(recorded.value.posthog_enabled).toBe(true);
  });

  test("persists no when user declines", async () => {
    const recorded = { value: null };
    const deps = makeDeps(makeConsentDeps({ promptResponse: false, recordedSave: recorded }));
    const ctx = {
      stdout: makeNoop(), stderr: makeNoop(),
      jsonMode: false, noOpen: true, noInput: false,
      env: {},
      preferences: { posthog_enabled: null },
    };
    const core = createCoreHandlers(ctx, { current_workspace_id: null, items: [] }, deps);
    await core.commandLogin([]);
    expect(recorded.value.posthog_enabled).toBe(false);
  });

  test("does not prompt when preferences already decided", async () => {
    const recorded = { value: null };
    let promptCalls = 0;
    const deps = makeDeps({
      ...makeConsentDeps({ recordedSave: recorded }),
      promptYesNo: async () => { promptCalls++; return true; },
    });
    const ctx = {
      stdout: makeNoop(), stderr: makeNoop(),
      jsonMode: false, noOpen: true, noInput: false,
      env: {},
      preferences: { posthog_enabled: true, decided_at: 1, schema_version: 1 },
    };
    const core = createCoreHandlers(ctx, { current_workspace_id: null, items: [] }, deps);
    await core.commandLogin([]);
    expect(promptCalls).toBe(0);
    expect(recorded.value).toBeNull();
  });

  test("does not prompt under --no-input", async () => {
    const recorded = { value: null };
    let promptCalls = 0;
    const deps = makeDeps({
      ...makeConsentDeps({ recordedSave: recorded }),
      promptYesNo: async () => { promptCalls++; return true; },
    });
    const ctx = {
      stdout: makeNoop(), stderr: makeNoop(),
      jsonMode: false, noOpen: true, noInput: true,
      env: {},
      preferences: { posthog_enabled: null },
    };
    const core = createCoreHandlers(ctx, { current_workspace_id: null, items: [] }, deps);
    await core.commandLogin([]);
    expect(promptCalls).toBe(0);
    expect(recorded.value).toBeNull();
  });

  test("does not prompt under --json", async () => {
    const recorded = { value: null };
    let promptCalls = 0;
    const deps = makeDeps({
      ...makeConsentDeps({ recordedSave: recorded }),
      promptYesNo: async () => { promptCalls++; return true; },
    });
    const ctx = {
      stdout: makeNoop(), stderr: makeNoop(),
      jsonMode: true, noOpen: true, noInput: false,
      env: {},
      preferences: { posthog_enabled: null },
    };
    const core = createCoreHandlers(ctx, { current_workspace_id: null, items: [] }, deps);
    await core.commandLogin([]);
    expect(promptCalls).toBe(0);
    expect(recorded.value).toBeNull();
  });

  test("does not prompt when ZOMBIE_POSTHOG_ENABLED env override present", async () => {
    const recorded = { value: null };
    let promptCalls = 0;
    const deps = makeDeps({
      ...makeConsentDeps({ recordedSave: recorded }),
      promptYesNo: async () => { promptCalls++; return true; },
    });
    const ctx = {
      stdout: makeNoop(), stderr: makeNoop(),
      jsonMode: false, noOpen: true, noInput: false,
      env: { ZOMBIE_POSTHOG_ENABLED: "false" },
      preferences: { posthog_enabled: null },
    };
    const core = createCoreHandlers(ctx, { current_workspace_id: null, items: [] }, deps);
    await core.commandLogin([]);
    expect(promptCalls).toBe(0);
    expect(recorded.value).toBeNull();
  });

  test("prompt returning null (Ctrl-C / non-TTY) does not write preferences", async () => {
    const recorded = { value: null };
    const deps = makeDeps({
      ...makeConsentDeps({ recordedSave: recorded }),
      promptYesNo: async () => null,
    });
    const ctx = {
      stdout: makeNoop(), stderr: makeNoop(),
      jsonMode: false, noOpen: true, noInput: false,
      env: {},
      preferences: { posthog_enabled: null },
    };
    const core = createCoreHandlers(ctx, { current_workspace_id: null, items: [] }, deps);
    const code = await core.commandLogin([]);
    expect(code).toBe(0);
    expect(recorded.value).toBeNull();
  });

  test("save failure surfaces stderr warning but login still succeeds", async () => {
    const err = makeBufferStream();
    const deps = makeDeps({
      request: async (_ctx, reqPath) => {
        if (reqPath === "/v1/auth/sessions") {
          return { session_id: "sess_save_fail", login_url: "https://login.test" };
        }
        return { status: "complete", token: "tok_save_fail" };
      },
      loadPreferences: async () => ({ posthog_enabled: null }),
      savePreferences: async () => {
        const e = new Error("permission denied");
        e.code = "EACCES";
        throw e;
      },
      promptYesNo: async () => true,
    });
    const ctx = {
      stdout: makeNoop(), stderr: err.stream,
      jsonMode: false, noOpen: true, noInput: false,
      env: {},
      preferences: { posthog_enabled: null },
    };
    const core = createCoreHandlers(ctx, { current_workspace_id: null, items: [] }, deps);
    const code = await core.commandLogin([]);
    expect(code).toBe(0);
    expect(err.read()).toContain("could not save telemetry preference");
  });
});
