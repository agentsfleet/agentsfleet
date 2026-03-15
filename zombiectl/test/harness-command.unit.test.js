import test from "node:test";
import assert from "node:assert/strict";
import { Writable } from "node:stream";
import { commandHarness } from "../src/commands/harness.js";

function bufferStream() {
  let data = "";
  return {
    stream: new Writable({
      write(chunk, _enc, cb) {
        data += String(chunk);
        cb();
      },
    }),
    read: () => data,
  };
}

function parseFlags(tokens) {
  const options = {};
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (!token.startsWith("--")) continue;
    const key = token.slice(2);
    const next = tokens[i + 1];
    if (next && !next.startsWith("--")) {
      options[key] = next;
      i += 1;
    } else {
      options[key] = true;
    }
  }
  return { options, positionals: [] };
}

test("commandHarness source put builds source_markdown payload", async () => {
  const out = bufferStream();
  const err = bufferStream();
  let captured = null;
  const deps = {
    parseFlags,
    request: async (_ctx, reqPath, options) => {
      captured = { reqPath, options };
      return { config_version_id: "pver_1" };
    },
    apiHeaders: () => ({ "Content-Type": "application/json" }),
    ui: { ok: (s) => s, err: (s) => s, info: (s) => s },
    printJson: () => {},
    writeLine: (stream, line = "") => stream.write(`${line}\n`),
    readFile: async () => "# Harness\n\n```json\n{\"profile_id\":\"ws_123-harness\",\"stages\":[]}\n```",
    resolvePath: (p) => p,
  };
  const ctx = { stdout: out.stream, stderr: err.stream, jsonMode: false };
  const workspaces = { current_workspace_id: "ws_123" };

  const code = await commandHarness(
    ctx,
    ["source", "put", "--file", "profile.md", "--profile-id", "ws_123-harness"],
    workspaces,
    deps,
  );
  assert.equal(code, 0);
  assert.equal(err.read(), "");
  assert.equal(captured.reqPath, "/v1/workspaces/ws_123/harness/source");
  const body = JSON.parse(captured.options.body);
  assert.equal(body.agent_id, "ws_123-harness");
  assert.equal(body.name, "profile");
  assert.match(body.source_markdown, /# Harness/);
});

test("commandHarness compile sends explicit profile selectors", async () => {
  let captured = null;
  const deps = {
    parseFlags,
    request: async (_ctx, reqPath, options) => {
      captured = { reqPath, options };
      return { compile_job_id: "cjob_1", is_valid: true };
    },
    apiHeaders: () => ({ "Content-Type": "application/json" }),
    ui: { ok: (s) => s, err: (s) => s, info: (s) => s },
    printJson: () => {},
    writeLine: () => {},
  };
  const ctx = { stdout: new Writable({ write(_c, _e, cb) { cb(); } }), stderr: new Writable({ write(_c, _e, cb) { cb(); } }), jsonMode: false };
  const workspaces = { current_workspace_id: "ws_123" };

  const code = await commandHarness(
    ctx,
    ["compile", "--profile-version-id", "pver_9"],
    workspaces,
    deps,
  );
  assert.equal(code, 0);
  assert.equal(captured.reqPath, "/v1/workspaces/ws_123/harness/compile");
  const body = JSON.parse(captured.options.body);
  assert.equal(body.agent_id, null);
  assert.equal(body.config_version_id, "pver_9");
});

test("commandHarness activate requires profile version id", async () => {
  const out = bufferStream();
  const err = bufferStream();
  const deps = {
    parseFlags,
    request: async () => {
      throw new Error("should not be called");
    },
    apiHeaders: () => ({ "Content-Type": "application/json" }),
    ui: { ok: (s) => s, err: (s) => s, info: (s) => s },
    printJson: () => {},
    writeLine: (stream, line = "") => stream.write(`${line}\n`),
  };
  const ctx = { stdout: out.stream, stderr: err.stream, jsonMode: false };
  const workspaces = { current_workspace_id: "ws_123" };

  const code = await commandHarness(ctx, ["activate"], workspaces, deps);
  assert.equal(code, 2);
  assert.match(err.read(), /requires --profile-version-id/);
});

test("commandHarness activate sends profile version and activated_by", async () => {
  let captured = null;
  const deps = {
    parseFlags,
    request: async (_ctx, reqPath, options) => {
      captured = { reqPath, options };
      return {
        agent_id: "ws_123-harness",
        config_version_id: "pver_2",
        run_snapshot_version: "pver_2",
        activated_at: 1730000000,
      };
    },
    apiHeaders: () => ({ "Content-Type": "application/json" }),
    ui: { ok: (s) => s, err: (s) => s, info: (s) => s },
    printJson: () => {},
    writeLine: () => {},
  };
  const ctx = { stdout: new Writable({ write(_c, _e, cb) { cb(); } }), stderr: new Writable({ write(_c, _e, cb) { cb(); } }), jsonMode: false };
  const workspaces = { current_workspace_id: "ws_123" };

  const code = await commandHarness(
    ctx,
    ["activate", "--profile-version-id", "pver_2", "--activated-by", "operator"],
    workspaces,
    deps,
  );
  assert.equal(code, 0);
  assert.equal(captured.reqPath, "/v1/workspaces/ws_123/harness/activate");
  const body = JSON.parse(captured.options.body);
  assert.equal(body.config_version_id, "pver_2");
  assert.equal(body.activated_by, "operator");
});

test("commandHarness active queries active profile endpoint", async () => {
  let captured = null;
  const deps = {
    parseFlags,
    request: async (_ctx, reqPath, options) => {
      captured = { reqPath, options };
      return {
        source: "active",
        agent_id: "ws_123-harness",
        config_version_id: "pver_2",
        run_snapshot_version: "pver_2",
      };
    },
    apiHeaders: () => ({ "Content-Type": "application/json" }),
    ui: { ok: (s) => s, err: (s) => s, info: (s) => s },
    printJson: () => {},
    writeLine: () => {},
  };
  const ctx = { stdout: new Writable({ write(_c, _e, cb) { cb(); } }), stderr: new Writable({ write(_c, _e, cb) { cb(); } }), jsonMode: false };
  const workspaces = { current_workspace_id: "ws_123" };

  const code = await commandHarness(ctx, ["active"], workspaces, deps);
  assert.equal(code, 0);
  assert.equal(captured.reqPath, "/v1/workspaces/ws_123/harness/active");
  assert.equal(captured.options.method, "GET");
});
