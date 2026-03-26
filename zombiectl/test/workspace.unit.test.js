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
      { workspace_id: WS_ID, repo_url: "https://github.com/acme/repo" },
      { workspace_id: WS_ID_2, repo_url: "https://github.com/acme/other" },
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

  test("remove by ID", async () => {
    let savedWs = null;
    const deps = makeDeps({
      saveWorkspaces: async (ws) => { savedWs = ws; },
    });
    const items = [{ workspace_id: WS_ID, repo_url: "https://github.com/acme/repo" }];
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [...items] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["remove", WS_ID]);
    expect(code).toBe(0);
    expect(savedWs.items.length).toBe(0);
  });

  test("remove updates current workspace", async () => {
    let savedWs = null;
    const deps = makeDeps({
      saveWorkspaces: async (ws) => { savedWs = ws; },
    });
    const items = [
      { workspace_id: WS_ID, repo_url: "https://github.com/acme/repo" },
      { workspace_id: WS_ID_2, repo_url: "https://github.com/acme/other" },
    ];
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [...items] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["remove", WS_ID]);
    expect(code).toBe(0);
    expect(savedWs.current_workspace_id).toBe(WS_ID_2);
  });

  test("remove without id returns error", async () => {
    const err = makeBufferStream();
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["remove"]);
    expect(code).toBe(2);
    expect(err.read()).toContain("workspace remove requires");
  });

  test("upgrade-scale requires workspace id", async () => {
    const err = makeBufferStream();
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["upgrade-scale"]);
    expect(code).toBe(2);
    expect(err.read()).toContain("workspace upgrade-scale requires --workspace-id");
  });

  test("upgrade-scale requires subscription id", async () => {
    const err = makeBufferStream();
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["upgrade-scale", "--workspace-id", WS_ID]);
    expect(code).toBe(2);
    expect(err.read()).toContain("workspace upgrade-scale requires --subscription-id");
  });

  test("upgrade-scale calls billing endpoint", async () => {
    const out = makeBufferStream();
    let called = null;
    const deps = makeDeps({
      request: async (_ctx, reqPath, options) => {
        called = { reqPath, options };
        return { plan_tier: "scale", billing_status: "active", subscription_id: "sub_scale_123" };
      },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["upgrade-scale", "--workspace-id", WS_ID, "--subscription-id", "sub_scale_123"]);
    expect(code).toBe(0);
    expect(called.reqPath).toContain(`/v1/workspaces/${WS_ID}/billing/scale`);
    expect(JSON.parse(called.options.body).subscription_id).toBe("sub_scale_123");
    expect(out.read()).toContain("workspace upgraded to scale");
  });

  test("upgrade-scale with subscription_id as second positional (both positional)", async () => {
    const out = makeBufferStream();
    let called = null;
    const deps = makeDeps({
      request: async (_ctx, reqPath, options) => {
        called = { reqPath, options };
        return { plan_tier: "scale", billing_status: "active", subscription_id: "sub_pos_456" };
      },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["upgrade-scale", WS_ID, "sub_pos_456"]);
    expect(code).toBe(0);
    expect(called.reqPath).toContain(`/v1/workspaces/${WS_ID}/billing/scale`);
    expect(JSON.parse(called.options.body).subscription_id).toBe("sub_pos_456");
    const output = out.read();
    expect(output).toContain("workspace upgraded to scale");
    expect(output).toContain("subscription_id: sub_pos_456");
  });

  test("upgrade-scale with --workspace-id flag and bare positional requires --subscription-id", async () => {
    const err = makeBufferStream();
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["upgrade-scale", "--workspace-id", WS_ID, "sub_pos_456"]);
    expect(code).toBe(2);
    expect(err.read()).toContain("requires --subscription-id");
  });

  test("upgrade-scale with null subscription_id in response omits subscription_id line", async () => {
    const out = makeBufferStream();
    const deps = makeDeps({
      request: async () => ({ plan_tier: "scale", billing_status: "active", subscription_id: null }),
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["upgrade-scale", "--workspace-id", WS_ID, "--subscription-id", "sub_input_789"]);
    expect(code).toBe(0);
    const output = out.read();
    expect(output).toContain("workspace upgraded to scale");
    expect(output).toContain("plan_tier: scale");
    expect(output).toContain("billing_status: active");
    expect(output).not.toContain("subscription_id:");
  });

  test("upgrade-scale in JSON mode prints JSON output", async () => {
    const apiResponse = { plan_tier: "scale", billing_status: "active", subscription_id: "sub_json_001" };
    let jsonOutput = null;
    const deps = makeDeps({
      request: async () => apiResponse,
      printJson: (_s, v) => { jsonOutput = v; },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: true, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandWorkspace(["upgrade-scale", "--workspace-id", WS_ID, "--subscription-id", "sub_json_001"]);
    expect(code).toBe(0);
    expect(jsonOutput).toEqual(apiResponse);
  });

  test("upgrade-scale when API request throws propagates error", async () => {
    const deps = makeDeps({
      request: async () => { throw new Error("network failure"); },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    await expect(
      core.commandWorkspace(["upgrade-scale", "--workspace-id", WS_ID, "--subscription-id", "sub_err_999"]),
    ).rejects.toThrow("network failure");
  });
});
