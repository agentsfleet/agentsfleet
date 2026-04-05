import { test } from "bun:test";
import assert from "node:assert/strict";
import { commandWorkspaceBillingSummary } from "../src/commands/workspace_billing_summary.js";
import { makeNoop, makeBufferStream, ui } from "./helpers.js";

const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-000000000099";

const SAMPLE_RESPONSE = {
  workspace_id: WORKSPACE_ID,
  period_days: 30,
  period_start_ms: 1700000000000,
  period_end_ms: 1702592000000,
  completed: { count: 42, agent_seconds: 1240 },
  non_billable: { count: 8 },
  non_billable_score_gated: { count: 3, avg_score: 27 },
  total_runs: 53,
};

// T1: Happy path — correct URL and period param
test("builds correct URL with default period", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return SAMPLE_RESPONSE; },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  const code = await commandWorkspaceBillingSummary(
    { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false },
    parsed, WORKSPACE_ID, deps,
  );
  assert.equal(code, 0);
  assert.match(calledUrl, new RegExp(`/v1/workspaces/${WORKSPACE_ID}/billing/summary`));
  assert.match(calledUrl, /period=30d/);
});

// T2: Custom period
test("passes custom period to URL", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return SAMPLE_RESPONSE; },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, writeLine: () => {},
  };
  const parsed = { options: { period: "7d" }, positionals: [] };
  await commandWorkspaceBillingSummary(
    { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false },
    parsed, WORKSPACE_ID, deps,
  );
  assert.match(calledUrl, /period=7d/);
});

// T3: JSON mode outputs raw response
test("json mode outputs raw response", async () => {
  let printed = null;
  const deps = {
    request: async () => SAMPLE_RESPONSE,
    apiHeaders: () => ({}),
    ui, printJson: (_stream, v) => { printed = v; }, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  const code = await commandWorkspaceBillingSummary(
    { stdout: makeNoop(), stderr: makeNoop(), jsonMode: true },
    parsed, WORKSPACE_ID, deps,
  );
  assert.equal(code, 0);
  assert.deepEqual(printed, SAMPLE_RESPONSE);
});

// T4: Human-readable output contains key fields
test("human output contains workspace and counts", async () => {
  const buf = makeBufferStream();
  const lines = [];
  const deps = {
    request: async () => SAMPLE_RESPONSE,
    apiHeaders: () => ({}),
    ui, printJson: () => {}, writeLine: (_stream, line) => { if (line != null) lines.push(String(line)); },
  };
  const parsed = { options: {}, positionals: [] };
  await commandWorkspaceBillingSummary(
    { stdout: buf.stream, stderr: makeNoop(), jsonMode: false },
    parsed, WORKSPACE_ID, deps,
  );
  const output = lines.join("\n");
  assert.match(output, new RegExp(WORKSPACE_ID));
  assert.match(output, /42/); // completed count
  assert.match(output, /1,240/); // agent_seconds formatted
  assert.match(output, /avg score: 27/);
  assert.match(output, /53/); // total
});

// T5: Empty billing data renders zeros, not errors
test("empty billing data renders table with zeros", async () => {
  const emptyResponse = {
    workspace_id: WORKSPACE_ID,
    period_days: 30,
    period_start_ms: 0,
    period_end_ms: 0,
    completed: { count: 0, agent_seconds: 0 },
    non_billable: { count: 0 },
    non_billable_score_gated: { count: 0, avg_score: 0 },
    total_runs: 0,
  };
  const lines = [];
  const deps = {
    request: async () => emptyResponse,
    apiHeaders: () => ({}),
    ui, printJson: () => {}, writeLine: (_stream, line) => { if (line != null) lines.push(String(line)); },
  };
  const parsed = { options: {}, positionals: [] };
  const code = await commandWorkspaceBillingSummary(
    { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false },
    parsed, WORKSPACE_ID, deps,
  );
  assert.equal(code, 0);
  assert.match(lines.join("\n"), /total runs/);
});
