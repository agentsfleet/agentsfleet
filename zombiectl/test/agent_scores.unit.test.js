import test from "node:test";
import assert from "node:assert/strict";
import { Writable } from "node:stream";
import { commandAgentScores } from "../src/commands/agent_scores.js";

function bufferStream() {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

const noop = new Writable({ write(_c, _e, cb) { cb(); } });
const ui = { ok: (s) => s, err: (s) => s, info: (s) => s, dim: (s) => s };

const sampleScores = [
  { score_id: "0195b4ba-8d3a-7f13-8abc-000000000001", run_id: "0195b4ba-8d3a-7f13-8abc-000000000011", score: 87, scored_at: 1700000002000 },
  { score_id: "0195b4ba-8d3a-7f13-8abc-000000000002", run_id: "0195b4ba-8d3a-7f13-8abc-000000000012", score: 72, scored_at: 1700000001000 },
];

test("commandAgentScores builds correct URL with limit", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return { data: [], has_more: false, next_cursor: null }; },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: { limit: 10 }, positionals: [] };
  const code = await commandAgentScores({ stdout: noop, stderr: noop, jsonMode: false }, parsed, "agent_1", deps);
  assert.equal(code, 0);
  assert.match(calledUrl, /limit=10/);
  assert.match(calledUrl, /\/v1\/agents\/agent_1\/scores/);
});

test("commandAgentScores appends starting_after to URL when provided", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return { data: [], has_more: false, next_cursor: null }; },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: { "starting-after": "score_cursor_id" }, positionals: [] };
  await commandAgentScores({ stdout: noop, stderr: noop, jsonMode: false }, parsed, "agent_1", deps);
  assert.match(calledUrl, /starting_after=score_cursor_id/);
});

test("commandAgentScores json mode outputs raw response", async () => {
  let printed = null;
  const res = { data: sampleScores, has_more: false, next_cursor: null };
  const deps = {
    request: async () => res,
    apiHeaders: () => ({}),
    ui, printJson: (_stream, v) => { printed = v; }, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  const code = await commandAgentScores({ stdout: noop, stderr: noop, jsonMode: true }, parsed, "agent_1", deps);
  assert.equal(code, 0);
  assert.deepEqual(printed, res);
});

test("commandAgentScores prints table in human mode", async () => {
  let tableRows = null;
  const deps = {
    request: async () => ({ data: sampleScores, has_more: false, next_cursor: null }),
    apiHeaders: () => ({}),
    ui, printJson: () => {},
    printTable: (_stream, _cols, rows) => { tableRows = rows; },
    writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentScores({ stdout: noop, stderr: noop, jsonMode: false }, parsed, "agent_1", deps);
  assert.equal(tableRows.length, 2);
});

test("commandAgentScores shows next_cursor hint when has_more", async () => {
  let output = "";
  const deps = {
    request: async () => ({ data: sampleScores, has_more: true, next_cursor: "cursor_abc" }),
    apiHeaders: () => ({}),
    ui, printJson: () => {},
    printTable: () => {},
    writeLine: (_stream, line = "") => { output += line; },
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentScores({ stdout: noop, stderr: noop, jsonMode: false }, parsed, "agent_1", deps);
  assert.match(output, /cursor_abc/);
});

test("commandAgentScores shows no scores message when data is empty", async () => {
  let output = "";
  const deps = {
    request: async () => ({ data: [], has_more: false, next_cursor: null }),
    apiHeaders: () => ({}),
    ui, printJson: () => {},
    printTable: () => {},
    writeLine: (_stream, line = "") => { output += line; },
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentScores({ stdout: noop, stderr: noop, jsonMode: false }, parsed, "agent_1", deps);
  assert.match(output, /no scores/);
});
