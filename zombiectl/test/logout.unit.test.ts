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
  Workspaces,
} from "../src/commands/types.ts";

function makeDeps(overrides: Partial<CommandDeps> = {}): CommandDeps {
  const base = {
    clearCredentials: async () => {},
    createSpinner: () => ({ start() {}, stop() {}, succeed() {}, fail() {} }),
    newIdempotencyKey: () => "idem_test",
    openUrl: async () => false,
    printJson: (_s: NodeJS.WritableStream, _v: unknown) => {},
    printKeyValue: () => {},
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

describe("commandLogout", () => {
  test("clears credentials", async () => {
    let cleared = false;
    const out = makeBufferStream();
    const deps = makeDeps({
      clearCredentials: async () => { cleared = true; },
    });
    const ctx: CommandCtx = {
      stdout: out.stream,
      stderr: makeNoop(),
      jsonMode: false,
      apiUrl: "https://api.test",
      env: {},
    };
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandLogout();
    expect(code).toBe(0);
    expect(cleared).toBe(true);
    expect(out.read()).toContain("logout complete");
  });

  test("JSON mode output", async () => {
    let printed: unknown = null;
    const deps = makeDeps({
      printJson: (_s, v) => { printed = v; },
    });
    const ctx: CommandCtx = {
      stdout: makeNoop(),
      stderr: makeNoop(),
      jsonMode: true,
      apiUrl: "https://api.test",
      env: {},
    };
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const core = createCoreHandlers(ctx, workspaces, deps);
    const code = await core.commandLogout();
    expect(code).toBe(0);
    expect(printed).toEqual({ status: "ok", logged_out: true });
  });
});
