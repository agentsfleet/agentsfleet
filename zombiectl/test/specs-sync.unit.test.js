import { describe, test, expect } from "bun:test";
import { makeNoop, makeBufferStream, ui, WS_ID } from "./helpers.js";
import { createCoreHandlers } from "../src/commands/core.js";
import { ApiError } from "../src/lib/http.js";
import { ERR_BILLING_CREDIT_EXHAUSTED } from "../src/constants/error-codes.js";

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
        return { synced_count: 3, total_pending: 0, plan_tier: "free", credit_remaining_cents: 1000, credit_currency: "USD" };
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
    expect(out.read()).toContain("credit_remaining_cents: 1000");
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

  test("prints upgrade path when free credit is exhausted", async () => {
    const out = makeBufferStream();
    const deps = makeDeps({
      request: async () => ({ synced_count: 0, total_pending: 2, plan_tier: "free", credit_remaining_cents: 0, credit_currency: "USD" }),
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandSpecsSync([]);
    expect(code).toBe(0);
    expect(out.read()).toContain("Upgrade path:");
    expect(out.read()).toContain("workspace upgrade-scale");
  });

  test("prints upgrade path when sync is rejected with credit exhausted", async () => {
    const err = makeBufferStream();
    const deps = makeDeps({
      request: async () => {
        throw new ApiError("Free plan credit exhausted. Upgrade to Scale to continue.", {
          status: 403,
          code: ERR_BILLING_CREDIT_EXHAUSTED,
        });
      },
    });
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);

    await expect(core.commandSpecsSync([])).rejects.toMatchObject({ code: ERR_BILLING_CREDIT_EXHAUSTED });
    expect(err.read()).toContain("Upgrade path:");
    expect(err.read()).toContain(`--workspace-id ${WS_ID}`);
  });

  test("JSON mode error path does NOT print upgrade hint on credit exhausted", async () => {
    const err = makeBufferStream();
    const deps = makeDeps({
      request: async () => {
        throw new ApiError("Free plan credit exhausted.", {
          status: 403,
          code: ERR_BILLING_CREDIT_EXHAUSTED,
        });
      },
    });
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: true, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);

    await expect(core.commandSpecsSync([])).rejects.toMatchObject({ code: ERR_BILLING_CREDIT_EXHAUSTED });
    expect(err.read()).not.toContain("Upgrade path:");
  });

  test("negative credit_remaining_cents triggers upgrade hint", async () => {
    const out = makeBufferStream();
    const deps = makeDeps({
      request: async () => ({ synced_count: 1, total_pending: 0, plan_tier: "free", credit_remaining_cents: -50, credit_currency: "USD" }),
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandSpecsSync([]);
    expect(code).toBe(0);
    expect(out.read()).toContain("Upgrade path:");
  });

  test("missing credit_remaining_cents does NOT trigger upgrade hint", async () => {
    const out = makeBufferStream();
    const deps = makeDeps({
      request: async () => ({ synced_count: 1, total_pending: 0, plan_tier: "free" }),
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandSpecsSync([]);
    expect(code).toBe(0);
    expect(out.read()).not.toContain("Upgrade path:");
  });
});
