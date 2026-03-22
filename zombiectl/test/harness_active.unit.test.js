import { test } from "bun:test";
import assert from "node:assert/strict";
import { commandHarnessActive } from "../src/commands/harness_active.js";
import { makeNoop, makeBufferStream, ui, PVER_ID } from "./helpers.js";

test("commandHarnessActive calls GET harness/active", async () => {
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
  const parsed = { options: {}, positionals: [] };
  const code = await commandHarnessActive({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, "ws_123", deps);
  assert.equal(code, 0);
  assert.equal(captured.reqPath, "/v1/workspaces/ws_123/harness/active");
  assert.equal(captured.options.method, "GET");
});

test("commandHarnessActive json mode outputs raw response", async () => {
  let printed = null;
  const deps = {
    request: async () => ({ agent_id: "agent_1", config_version_id: PVER_ID, run_snapshot_version: PVER_ID }),
    apiHeaders: () => ({}),
    ui,
    printJson: (_stream, v) => { printed = v; },
    writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  const code = await commandHarnessActive({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: true }, parsed, "ws_123", deps);
  assert.equal(code, 0);
  assert.equal(printed.config_version_id, PVER_ID);
});

test("commandHarnessActive falls back to default-v1 for null fields", async () => {
  let output = "";
  const deps = {
    request: async () => ({ agent_id: null, config_version_id: null, run_snapshot_version: null }),
    apiHeaders: () => ({}),
    ui,
    printJson: () => {},
    printSection: (_stream, title) => { output += title; },
    printKeyValue: (_stream, rows) => { output += Object.values(rows).join(" "); },
  };
  const parsed = { options: {}, positionals: [] };
  await commandHarnessActive({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, "ws_123", deps);
  assert.match(output, /default-v1/);
});
