import test from "node:test";
import assert from "node:assert/strict";
import { Writable } from "node:stream";
import { commandHarnessActivate } from "../src/commands/harness_activate.js";

function bufferStream() {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

const noop = new Writable({ write(_c, _e, cb) { cb(); } });
const ui = { ok: (s) => s, err: (s) => s };

test("commandHarnessActivate returns 2 when profile-version-id is missing", async () => {
  const err = bufferStream();
  const deps = { ui, writeLine: (stream, line = "") => stream.write(`${line}\n`) };
  const parsed = { options: {}, positionals: [] };
  const code = await commandHarnessActivate({ stdout: noop, stderr: err.stream, jsonMode: false }, parsed, "ws_123", deps);
  assert.equal(code, 2);
  assert.match(err.read(), /--profile-version-id/);
});

test("commandHarnessActivate sends profile_version_id and activated_by", async () => {
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
  const parsed = { options: { "profile-version-id": "pver_2", "activated-by": "operator" }, positionals: [] };
  const code = await commandHarnessActivate({ stdout: noop, stderr: noop, jsonMode: false }, parsed, "ws_123", deps);
  assert.equal(code, 0);
  assert.equal(captured.reqPath, "/v1/workspaces/ws_123/harness/activate");
  const body = JSON.parse(captured.options.body);
  assert.equal(body.profile_version_id, "pver_2");
  assert.equal(body.activated_by, "operator");
});

test("commandHarnessActivate defaults activated_by to zombiectl", async () => {
  let body = null;
  const deps = {
    request: async (_ctx, _p, options) => { body = JSON.parse(options.body); return {}; },
    apiHeaders: () => ({}),
    ui,
    printJson: () => {},
    writeLine: () => {},
  };
  const parsed = { options: { "profile-version-id": "pver_3" }, positionals: [] };
  await commandHarnessActivate({ stdout: noop, stderr: noop, jsonMode: false }, parsed, "ws_123", deps);
  assert.equal(body.activated_by, "zombiectl");
});
