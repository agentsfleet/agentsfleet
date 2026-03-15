import test from "node:test";
import assert from "node:assert/strict";
import { Writable } from "node:stream";
import { commandHarnessActive } from "../src/commands/harness_active.js";

const noop = new Writable({ write(_c, _e, cb) { cb(); } });
const ui = { ok: (s) => s, err: (s) => s, info: (s) => s };

test("commandHarnessActive calls GET harness/active", async () => {
  let captured = null;
  const deps = {
    request: async (_ctx, reqPath, options) => {
      captured = { reqPath, options };
      return { profile_id: "agent_1", profile_version_id: "pver_2", run_snapshot_version: "pver_2" };
    },
    apiHeaders: () => ({}),
    ui,
    printJson: () => {},
    writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  const code = await commandHarnessActive({ stdout: noop, stderr: noop, jsonMode: false }, parsed, "ws_123", deps);
  assert.equal(code, 0);
  assert.equal(captured.reqPath, "/v1/workspaces/ws_123/harness/active");
  assert.equal(captured.options.method, "GET");
});

test("commandHarnessActive json mode outputs raw response", async () => {
  let printed = null;
  const deps = {
    request: async () => ({ profile_id: "agent_1", profile_version_id: "pver_2", run_snapshot_version: "pver_2" }),
    apiHeaders: () => ({}),
    ui,
    printJson: (_stream, v) => { printed = v; },
    writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  const code = await commandHarnessActive({ stdout: noop, stderr: noop, jsonMode: true }, parsed, "ws_123", deps);
  assert.equal(code, 0);
  assert.equal(printed.profile_version_id, "pver_2");
});

test("commandHarnessActive falls back to default-v1 for null fields", async () => {
  let output = "";
  const deps = {
    request: async () => ({ profile_id: null, profile_version_id: null, run_snapshot_version: null }),
    apiHeaders: () => ({}),
    ui,
    printJson: () => {},
    writeLine: (_stream, line = "") => { output += line; },
  };
  const parsed = { options: {}, positionals: [] };
  await commandHarnessActive({ stdout: noop, stderr: noop, jsonMode: false }, parsed, "ws_123", deps);
  assert.match(output, /default-v1/);
});
