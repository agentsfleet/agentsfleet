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

describe("commandLogout", () => {
  test("clears credentials", async () => {
    let cleared = false;
    const out = makeBufferStream();
    const deps = makeDeps({
      clearCredentials: async () => { cleared = true; },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandLogout();
    expect(code).toBe(0);
    expect(cleared).toBe(true);
    expect(out.read()).toContain("logout complete");
  });

  test("JSON mode output", async () => {
    let printed = null;
    const deps = makeDeps({
      printJson: (_s, v) => { printed = v; },
    });
    const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: true, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandLogout();
    expect(code).toBe(0);
    expect(printed).toEqual({ status: "ok", logged_out: true });
  });
});
