import { describe, test, expect } from "bun:test";
import { makeNoop, makeBufferStream, ui, WS_ID } from "./helpers.js";
import { createCoreOpsHandlers } from "../src/commands/core-ops.js";

function makeDeps(overrides = {}) {
  return {
    apiHeaders: () => ({}),
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
    request: async () => ({}),
    ui,
    writeLine: (stream, line = "") => stream.write(`${line}\n`),
    ...overrides,
  };
}

describe("commandSkillSecret", () => {
  test("put with all flags", async () => {
    const out = makeBufferStream();
    let calledPath = null;
    let calledBody = null;
    const deps = makeDeps({
      request: async (_ctx, reqPath, opts) => {
        calledPath = reqPath;
        calledBody = JSON.parse(opts.body);
        return { ok: true };
      },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const ops = createCoreOpsHandlers(ctx, workspaces, deps);
    const code = await ops.commandSkillSecret([
      "put",
      "--workspace-id", WS_ID,
      "--skill-ref", "my-skill",
      "--key", "API_KEY",
      "--value", "sk-secret",
      "--scope", "host",
    ]);
    expect(code).toBe(0);
    expect(calledPath).toContain(WS_ID);
    expect(calledPath).toContain("my-skill");
    expect(calledPath).toContain("API_KEY");
    expect(calledBody.value).toBe("sk-secret");
    expect(calledBody.scope).toBe("host");
  });

  test("delete", async () => {
    const out = makeBufferStream();
    let calledMethod = null;
    const deps = makeDeps({
      request: async (_ctx, _reqPath, opts) => {
        calledMethod = opts.method;
        return { ok: true };
      },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [] };
    const ops = createCoreOpsHandlers(ctx, workspaces, deps);
    const code = await ops.commandSkillSecret([
      "delete",
      "--workspace-id", WS_ID,
      "--skill-ref", "my-skill",
      "--key", "API_KEY",
    ]);
    expect(code).toBe(0);
    expect(calledMethod).toBe("DELETE");
    expect(out.read()).toContain("skill secret deleted");
  });

  test("missing required flags error", async () => {
    const err = makeBufferStream();
    const deps = makeDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: null, items: [] };
    const ops = createCoreOpsHandlers(ctx, workspaces, deps);
    const code = await ops.commandSkillSecret(["put"]);
    expect(code).toBe(2);
    expect(err.read()).toContain("--workspace-id");
    expect(err.read()).toContain("--skill-ref");
  });
});
