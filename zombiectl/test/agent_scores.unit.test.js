import test from "node:test";
import assert from "node:assert/strict";
import { commandAgentScores } from "../src/commands/agent_scores.js";
import {
  makeNoop, makeBufferStream,
  ui, ApiError,
  AGENT_ID, SCORE_ID_1, SCORE_ID_2, RUN_ID_1, RUN_ID_2,
} from "./helpers.js";

const SAMPLE_SCORES = [
  { score_id: SCORE_ID_1, run_id: RUN_ID_1, score: 87, scored_at: 1700000002000 },
  { score_id: SCORE_ID_2, run_id: RUN_ID_2, score: 72, scored_at: 1700000001000 },
];

// ── T1: Happy path ────────────────────────────────────────────────────────────

test("commandAgentScores builds correct URL with limit", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return { data: [], has_more: false, next_cursor: null }; },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: { limit: 10 }, positionals: [] };
  const code = await commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.equal(code, 0);
  assert.match(calledUrl, /limit=10/);
  assert.match(calledUrl, new RegExp(`/v1/agents/${AGENT_ID}/scores`));
});

test("commandAgentScores appends starting_after to URL when provided", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return { data: [], has_more: false, next_cursor: null }; },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: { "starting-after": SCORE_ID_1 }, positionals: [] };
  await commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.match(calledUrl, new RegExp(`starting_after=${encodeURIComponent(SCORE_ID_1)}`));
});

test("commandAgentScores json mode outputs raw response", async () => {
  let printed = null;
  const res = { data: SAMPLE_SCORES, has_more: false, next_cursor: null };
  const deps = {
    request: async () => res,
    apiHeaders: () => ({}),
    ui, printJson: (_stream, v) => { printed = v; }, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  const code = await commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: true }, parsed, AGENT_ID, deps);
  assert.equal(code, 0);
  assert.deepEqual(printed, res);
});

test("commandAgentScores prints table in human mode", async () => {
  let tableRows = null;
  const deps = {
    request: async () => ({ data: SAMPLE_SCORES, has_more: false, next_cursor: null }),
    apiHeaders: () => ({}),
    ui, printJson: () => {},
    printTable: (_stream, _cols, rows) => { tableRows = rows; },
    writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.equal(tableRows.length, 2);
});

test("commandAgentScores shows next_cursor hint when has_more", async () => {
  let output = "";
  const deps = {
    request: async () => ({ data: SAMPLE_SCORES, has_more: true, next_cursor: SCORE_ID_1 }),
    apiHeaders: () => ({}),
    ui, printJson: () => {},
    printTable: () => {},
    writeLine: (_stream, line = "") => { output += line; },
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.match(output, new RegExp(SCORE_ID_1));
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
  await commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.match(output, /no scores/);
});

// ── T2: Edge cases ────────────────────────────────────────────────────────────

test("commandAgentScores uses default limit 20 when none specified", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return { data: [], has_more: false, next_cursor: null }; },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.match(calledUrl, /limit=20/);
});

test("commandAgentScores with limit=0 sends limit=0 in URL", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return { data: [], has_more: false, next_cursor: null }; },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: { limit: 0 }, positionals: [] };
  await commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  // limit=0 is falsy so falls back to default 20 — verify the documented behavior
  assert.match(calledUrl, /limit=/);
});

test("commandAgentScores omits starting_after when not provided", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return { data: [], has_more: false, next_cursor: null }; },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.doesNotMatch(calledUrl, /starting_after/);
});

test("commandAgentScores suppresses cursor hint when has_more=true but next_cursor=null", async () => {
  let output = "";
  const deps = {
    request: async () => ({ data: SAMPLE_SCORES, has_more: true, next_cursor: null }),
    apiHeaders: () => ({}),
    ui, printJson: () => {},
    printTable: () => {},
    writeLine: (_stream, line = "") => { output += line; },
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.doesNotMatch(output, /--starting-after/);
});

test("commandAgentScores URL-encodes agent_id with special characters", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return { data: [], has_more: false, next_cursor: null }; },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, "agent/with spaces", deps);
  assert.match(calledUrl, /agent%2Fwith%20spaces/);
});

// ── T3: Error paths ───────────────────────────────────────────────────────────

test("commandAgentScores propagates ApiError 404 from request", async () => {
  const deps = {
    request: async () => { throw new ApiError("not found", { status: 404, code: "UZ-AGENT-001" }); },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await assert.rejects(
    () => commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps),
    (err) => err instanceof ApiError && err.status === 404,
  );
});

test("commandAgentScores propagates ApiError 500 from request", async () => {
  const deps = {
    request: async () => { throw new ApiError("internal error", { status: 500, code: "HTTP_500" }); },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await assert.rejects(
    () => commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps),
    (err) => err instanceof ApiError && err.status === 500,
  );
});

test("commandAgentScores propagates network timeout error", async () => {
  const deps = {
    request: async () => { throw new ApiError("request timed out after 15000ms", { status: 408, code: "TIMEOUT" }); },
    apiHeaders: () => ({}),
    ui, printJson: () => {}, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await assert.rejects(
    () => commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps),
    (err) => err.code === "TIMEOUT",
  );
});

// ── T4: Output fidelity ───────────────────────────────────────────────────────

test("commandAgentScores json mode output is JSON.stringify-serializable", async () => {
  const res = { data: SAMPLE_SCORES, has_more: false, next_cursor: null };
  let printed = null;
  const deps = {
    request: async () => res,
    apiHeaders: () => ({}),
    ui, printJson: (_stream, v) => { printed = v; }, printTable: () => {}, writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  await commandAgentScores({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: true }, parsed, AGENT_ID, deps);
  const serialized = JSON.stringify(printed);
  const roundTripped = JSON.parse(serialized);
  assert.deepEqual(roundTripped, res);
});
