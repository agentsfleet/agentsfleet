import test from "node:test";
import assert from "node:assert/strict";
import { commandAgentProfile } from "../src/commands/agent_profile.js";
import {
  makeNoop,
  ui, ApiError,
  AGENT_ID, AGENT_NAME, WS_ID,
} from "./helpers.js";

const SAMPLE_AGENT = {
  agent_id:     AGENT_ID,
  name:         AGENT_NAME,
  status:       "ACTIVE",
  workspace_id: WS_ID,
  created_at:   1700000000000,
  updated_at:   1700000001000,
};

// ── T1: Happy path ────────────────────────────────────────────────────────────

test("commandAgentProfile calls GET /v1/agents/{agent_id}", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return SAMPLE_AGENT; },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printKeyValue: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  const code = await commandAgentProfile({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.equal(code, 0);
  assert.match(calledUrl, new RegExp(AGENT_ID));
});

test("commandAgentProfile json mode outputs raw response", async () => {
  let printed = null;
  const deps = {
    request: async () => SAMPLE_AGENT,
    apiHeaders: () => ({}),
    ui, printJson: (_stream, v) => { printed = v; }, printKeyValue: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  const code = await commandAgentProfile({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: true }, parsed, AGENT_ID, deps);
  assert.equal(code, 0);
  assert.deepEqual(printed, SAMPLE_AGENT);
});

test("commandAgentProfile human mode calls printKeyValue with agent fields", async () => {
  let kvData = null;
  const deps = {
    request: async () => SAMPLE_AGENT,
    apiHeaders: () => ({}),
    ui, printJson: () => {},
    printKeyValue: (_stream, v) => { kvData = v; },
    writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentProfile({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.equal(kvData.agent_id, SAMPLE_AGENT.agent_id);
  assert.equal(kvData.name, SAMPLE_AGENT.name);
  assert.equal(kvData.status, SAMPLE_AGENT.status);
});

// ── T2: Edge cases ────────────────────────────────────────────────────────────

test("commandAgentProfile URL-encodes agent_id with special characters", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return SAMPLE_AGENT; },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printKeyValue: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentProfile({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, "agent/with spaces", deps);
  assert.match(calledUrl, /agent%2Fwith%20spaces/);
});

test("commandAgentProfile human mode includes all expected keys", async () => {
  let kvData = null;
  const deps = {
    request: async () => SAMPLE_AGENT,
    apiHeaders: () => ({}),
    ui, printJson: () => {},
    printKeyValue: (_stream, v) => { kvData = v; },
    writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentProfile({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.ok(Object.prototype.hasOwnProperty.call(kvData, "workspace_id"), "missing workspace_id");
  assert.ok(Object.prototype.hasOwnProperty.call(kvData, "created_at"),  "missing created_at");
  assert.ok(Object.prototype.hasOwnProperty.call(kvData, "updated_at"),  "missing updated_at");
});

// ── T3: Error paths ───────────────────────────────────────────────────────────

test("commandAgentProfile propagates ApiError 404 when agent not found", async () => {
  const deps = {
    request: async () => { throw new ApiError("not found", { status: 404, code: "UZ-AGENT-001" }); },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printKeyValue: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await assert.rejects(
    () => commandAgentProfile({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps),
    (err) => err instanceof ApiError && err.status === 404,
  );
});

test("commandAgentProfile propagates ApiError 403 when forbidden", async () => {
  const deps = {
    request: async () => { throw new ApiError("forbidden", { status: 403, code: "HTTP_403" }); },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printKeyValue: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await assert.rejects(
    () => commandAgentProfile({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps),
    (err) => err instanceof ApiError && err.status === 403,
  );
});

test("commandAgentProfile propagates network timeout error", async () => {
  const deps = {
    request: async () => { throw new ApiError("request timed out after 15000ms", { status: 408, code: "TIMEOUT" }); },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printKeyValue: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await assert.rejects(
    () => commandAgentProfile({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps),
    (err) => err.code === "TIMEOUT",
  );
});

// ── T4: Output fidelity ───────────────────────────────────────────────────────

test("commandAgentProfile json mode output is JSON.stringify-serializable", async () => {
  let printed = null;
  const deps = {
    request: async () => SAMPLE_AGENT,
    apiHeaders: () => ({}),
    ui, printJson: (_stream, v) => { printed = v; }, printKeyValue: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentProfile({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: true }, parsed, AGENT_ID, deps);
  const roundTripped = JSON.parse(JSON.stringify(printed));
  assert.deepEqual(roundTripped, SAMPLE_AGENT);
});
