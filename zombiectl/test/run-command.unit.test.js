// run-command.unit.test.js — covers M63_004 §3 runCommand wrapper.
// Pins: started/finished event order, errorMap dispatch, propagation
// of retry config to ctx.retryConfig, fetch-failed → API_UNREACHABLE,
// unknown → UNEXPECTED, exit-code mapping.

import { test } from "bun:test";
import assert from "node:assert/strict";
import { ApiError } from "../src/lib/http.js";
import { runCommand, runCommandInternals } from "../src/lib/run-command.js";

function captureEvents() {
  const events = [];
  const trackCliEvent = (_client, _id, event, props) => events.push({ event, props });
  return { events, trackCliEvent };
}

function captureWriter() {
  const lines = [];
  const jsonPayloads = [];
  const writeLine = (_stream, line = "") => lines.push(line);
  const printJson = (_stream, value) => jsonPayloads.push(value);
  const ui = { err: (s) => s };
  return { lines, jsonPayloads, writeLine, printJson, ui };
}

const STDERR_STUB = { write: () => {} };

test("runCommand: success path emits started → finished, returns handler exit code", async () => {
  const { events, trackCliEvent } = captureEvents();
  const code = await runCommand({
    name: "test-cmd",
    handler: async () => 0,
    ctx: { jsonMode: false, apiUrl: "http://x" },
    deps: { trackCliEvent },
  });
  assert.equal(code, 0);
  const eventNames = events.map((e) => e.event);
  assert.deepEqual(eventNames, ["cli_command_started", "cli_command_finished"]);
  assert.equal(events[0].props.command, "test-cmd");
  assert.equal(events[1].props.exit_code, "0");
});

test("runCommand: handler returning non-number normalizes to exit 0", async () => {
  const { events, trackCliEvent } = captureEvents();
  const code = await runCommand({
    name: "test-cmd",
    handler: async () => undefined,
    ctx: {},
    deps: { trackCliEvent },
  });
  assert.equal(code, 0);
  assert.equal(events.at(-1).event, "cli_command_finished");
});

test("runCommand: ApiError prints `error: <code> <message>` + request_id, emits cli_error, returns 1", async () => {
  const { events, trackCliEvent } = captureEvents();
  const w = captureWriter();
  const code = await runCommand({
    name: "boom",
    handler: async () => { throw new ApiError("nope", { status: 400, code: "UZ-VAL-001", requestId: "r1" }); },
    ctx: { stderr: STDERR_STUB, jsonMode: false },
    deps: { trackCliEvent, writeLine: w.writeLine, printJson: w.printJson, ui: w.ui },
  });
  assert.equal(code, 1);
  const last = events.at(-1);
  assert.equal(last.event, "cli_error");
  assert.equal(last.props.error_code, "UZ-VAL-001");
  assert.deepEqual(w.lines, ["error: UZ-VAL-001 nope", "request_id: r1"]);
});

test("runCommand: ApiError in jsonMode prints structured envelope", async () => {
  const { trackCliEvent } = captureEvents();
  const w = captureWriter();
  const code = await runCommand({
    name: "boom",
    handler: async () => { throw new ApiError("nope", { status: 404, code: "UZ-WS-404", requestId: "r2" }); },
    ctx: { stderr: STDERR_STUB, jsonMode: true },
    deps: { trackCliEvent, writeLine: w.writeLine, printJson: w.printJson, ui: w.ui },
  });
  assert.equal(code, 1);
  assert.equal(w.lines.length, 0);
  assert.deepEqual(w.jsonPayloads, [{
    error: { code: "UZ-WS-404", message: "nope", status: 404, request_id: "r2" },
  }]);
});

test("runCommand: errorMap remaps ApiError code/message in render + analytics", async () => {
  const { events, trackCliEvent } = captureEvents();
  const w = captureWriter();
  const code = await runCommand({
    name: "boom",
    handler: async () => { throw new ApiError("raw msg", { status: 400, code: "UZ-VAL-001" }); },
    errorMap: {
      "UZ-VAL-001": { code: "WORKSPACE_NAME_INVALID", message: "Pick a different name." },
    },
    ctx: { stderr: STDERR_STUB, jsonMode: false },
    deps: { trackCliEvent, writeLine: w.writeLine, printJson: w.printJson, ui: w.ui },
  });
  assert.equal(code, 1);
  assert.equal(w.lines[0], "error: WORKSPACE_NAME_INVALID Pick a different name.");
  assert.equal(events.at(-1).props.error_code, "WORKSPACE_NAME_INVALID");
});

test("runCommand: TypeError('fetch failed') → API_UNREACHABLE, exit 1", async () => {
  const { events, trackCliEvent } = captureEvents();
  const w = captureWriter();
  const code = await runCommand({
    name: "doctor",
    handler: async () => { throw new TypeError("fetch failed"); },
    ctx: { stderr: STDERR_STUB, jsonMode: false, apiUrl: "http://api.example.com" },
    deps: { trackCliEvent, writeLine: w.writeLine, printJson: w.printJson, ui: w.ui },
  });
  assert.equal(code, 1);
  assert.match(w.lines[0], /cannot reach usezombie API at http:\/\/api\.example\.com/);
  assert.equal(events.at(-1).props.error_code, "API_UNREACHABLE");
});

test("runCommand: unknown throw → UNEXPECTED, exit 1, no double-throw", async () => {
  const { events, trackCliEvent } = captureEvents();
  const w = captureWriter();
  const code = await runCommand({
    name: "kaboom",
    handler: async () => { throw new Error("kaboom"); },
    ctx: { stderr: STDERR_STUB, jsonMode: false },
    deps: { trackCliEvent, writeLine: w.writeLine, printJson: w.printJson, ui: w.ui },
  });
  assert.equal(code, 1);
  assert.equal(w.lines[0], "kaboom");
  assert.equal(events.at(-1).props.error_code, "UNEXPECTED");
});

test("runCommand: retry=false sets ctx.retryConfig.maxAttempts=1 (propagation)", async () => {
  let observed = null;
  await runCommand({
    name: "obs",
    retry: false,
    handler: async (ctx) => { observed = ctx.retryConfig; return 0; },
    ctx: { jsonMode: false },
    deps: { trackCliEvent: () => {} },
  });
  assert.deepEqual(observed, { maxAttempts: 1 });
});

test("runCommand: retry={maxAttempts:5} propagates verbatim", async () => {
  let observed = null;
  await runCommand({
    name: "obs",
    retry: { maxAttempts: 5 },
    handler: async (ctx) => { observed = ctx.retryConfig; return 0; },
    ctx: {},
    deps: { trackCliEvent: () => {} },
  });
  assert.deepEqual(observed, { maxAttempts: 5 });
});

test("runCommand: retry undefined → ctx.retryConfig is null (defer to apiRequestWithRetry default)", async () => {
  let observed = "untouched";
  await runCommand({
    name: "obs",
    handler: async (ctx) => { observed = ctx.retryConfig; return 0; },
    ctx: {},
    deps: { trackCliEvent: () => {} },
  });
  assert.equal(observed, null);
});

test("runCommand: retry=true is treated like undefined (defer to default)", async () => {
  let observed = "untouched";
  await runCommand({
    name: "obs",
    retry: true,
    handler: async (ctx) => { observed = ctx.retryConfig; return 0; },
    ctx: {},
    deps: { trackCliEvent: () => {} },
  });
  assert.equal(observed, null);
});

test("runCommand: instrument=false suppresses started/finished/error events", async () => {
  const { events, trackCliEvent } = captureEvents();
  const code = await runCommand({
    name: "silent",
    instrument: false,
    handler: async () => 0,
    ctx: {},
    deps: { trackCliEvent },
  });
  assert.equal(code, 0);
  assert.equal(events.length, 0);
});

test("runCommand: bad inputs throw TypeError synchronously", async () => {
  await assert.rejects(() => runCommand({ name: "x" }), TypeError);
  await assert.rejects(() => runCommand({ handler: async () => 0 }), TypeError);
  await assert.rejects(() => runCommand({ name: "", handler: async () => 0 }), TypeError);
});

test("resolveRetryConfig: covers all four input shapes", () => {
  assert.equal(runCommandInternals.resolveRetryConfig(undefined), null);
  assert.equal(runCommandInternals.resolveRetryConfig(true), null);
  assert.deepEqual(runCommandInternals.resolveRetryConfig(false), { maxAttempts: 1 });
  assert.deepEqual(runCommandInternals.resolveRetryConfig({ maxAttempts: 7 }), { maxAttempts: 7 });
});
