import test from "node:test";
import assert from "node:assert/strict";
import { Writable } from "node:stream";
import { commandAgentProfile } from "../src/commands/agent_profile.js";

const noop = new Writable({ write(_c, _e, cb) { cb(); } });
const ui = { ok: (s) => s, err: (s) => s };

const sampleAgent = {
  agent_id: "0195b4ba-8d3a-7f13-8abc-000000000001",
  name: "my-agent",
  status: "ACTIVE",
  workspace_id: "0195b4ba-8d3a-7f13-8abc-000000000010",
  created_at: 1700000000000,
  updated_at: 1700000001000,
};

test("commandAgentProfile calls GET /v1/agents/{agent_id}", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return sampleAgent; },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printKeyValue: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  const code = await commandAgentProfile({ stdout: noop, stderr: noop, jsonMode: false }, parsed, sampleAgent.agent_id, deps);
  assert.equal(code, 0);
  assert.match(calledUrl, new RegExp(sampleAgent.agent_id));
});

test("commandAgentProfile json mode outputs raw response", async () => {
  let printed = null;
  const deps = {
    request: async () => sampleAgent,
    apiHeaders: () => ({}),
    ui, printJson: (_stream, v) => { printed = v; }, printKeyValue: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  const code = await commandAgentProfile({ stdout: noop, stderr: noop, jsonMode: true }, parsed, sampleAgent.agent_id, deps);
  assert.equal(code, 0);
  assert.deepEqual(printed, sampleAgent);
});

test("commandAgentProfile human mode calls printKeyValue with agent fields", async () => {
  let kvData = null;
  const deps = {
    request: async () => sampleAgent,
    apiHeaders: () => ({}),
    ui, printJson: () => {},
    printKeyValue: (_stream, v) => { kvData = v; },
    writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentProfile({ stdout: noop, stderr: noop, jsonMode: false }, parsed, sampleAgent.agent_id, deps);
  assert.equal(kvData.agent_id, sampleAgent.agent_id);
  assert.equal(kvData.name, sampleAgent.name);
  assert.equal(kvData.status, sampleAgent.status);
});
