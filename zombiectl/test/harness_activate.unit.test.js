import { test } from "bun:test";
import assert from "node:assert/strict";
import { commandHarnessActivate } from "../src/commands/harness_activate.js";
import { makeNoop, makeBufferStream, ui, PVER_ID } from "./helpers.js";

test("commandHarnessActivate returns 2 when profile-version-id is missing", async () => {
  const err = makeBufferStream();
  const deps = { ui, writeLine: (stream, line = "") => stream.write(`${line}\n`) };
  const parsed = { options: {}, positionals: [] };
  const code = await commandHarnessActivate({ stdout: makeNoop(), stderr: err.stream, jsonMode: false }, parsed, "ws_123", deps);
  assert.equal(code, 2);
  assert.match(err.read(), /--config-version-id/);
});

test("commandHarnessActivate sends profile_version_id and activated_by", async () => {
  let captured = null;
  const deps = {
    request: async (_ctx, reqPath, options) => {
      captured = { reqPath, options };
      return { agent_id: "agent_1", config_version_id: PVER_ID, run_snapshot_version: PVER_ID };
    },
    apiHeaders: () => ({}),
    ui,
    printJson: () => {},
    writeLine: () => {},
  };
  const parsed = { options: { "config-version-id": PVER_ID, "activated-by": "operator" }, positionals: [] };
  const code = await commandHarnessActivate({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, "ws_123", deps);
  assert.equal(code, 0);
  assert.equal(captured.reqPath, "/v1/workspaces/ws_123/harness/activate");
  const body = JSON.parse(captured.options.body);
  assert.equal(body.config_version_id, PVER_ID);
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
  const parsed = { options: { "config-version-id": PVER_ID }, positionals: [] };
  await commandHarnessActivate({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, "ws_123", deps);
  assert.equal(body.activated_by, "zombiectl");
});
