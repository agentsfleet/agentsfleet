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

test("runCommand: ApiError prints, emits cli_error, returns 1", async () => {
  const { events, trackCliEvent } = captureEvents();
  let written = null;
  const code = await runCommand({
    name: "boom",
    handler: async () => { throw new ApiError("nope", { status: 400, code: "UZ-VAL-001", requestId: "r1" }); },
    ctx: {},
    deps: { trackCliEvent, writeError: (info) => { written = info; } },
  });
  assert.equal(code, 1);
  const last = events.at(-1);
  assert.equal(last.event, "cli_error");
  assert.equal(last.props.error_code, "UZ-VAL-001");
  assert.equal(written.code, "UZ-VAL-001");
  assert.equal(written.message, "nope");
  assert.equal(written.requestId, "r1");
});

test("runCommand: errorMap remaps ApiError code/message in print + analytics", async () => {
  const { events, trackCliEvent } = captureEvents();
  let written = null;
  const code = await runCommand({
    name: "boom",
    handler: async () => { throw new ApiError("raw msg", { status: 400, code: "UZ-VAL-001" }); },
    errorMap: {
      "UZ-VAL-001": { code: "WORKSPACE_NAME_INVALID", message: "Pick a different name." },
    },
    ctx: {},
    deps: { trackCliEvent, writeError: (info) => { written = info; } },
  });
  assert.equal(code, 1);
  assert.equal(written.code, "WORKSPACE_NAME_INVALID");
  assert.equal(written.message, "Pick a different name.");
  assert.equal(events.at(-1).props.error_code, "WORKSPACE_NAME_INVALID");
});

test("runCommand: TypeError('fetch failed') → API_UNREACHABLE, exit 1", async () => {
  const { events, trackCliEvent } = captureEvents();
  let written = null;
  const code = await runCommand({
    name: "doctor",
    handler: async () => { throw new TypeError("fetch failed"); },
    ctx: { apiUrl: "http://api.example.com" },
    deps: { trackCliEvent, writeError: (info) => { written = info; } },
  });
  assert.equal(code, 1);
  assert.equal(written.code, "API_UNREACHABLE");
  assert.match(written.message, /cannot reach usezombie API at http:\/\/api\.example\.com/);
  assert.equal(events.at(-1).props.error_code, "API_UNREACHABLE");
});

test("runCommand: unknown throw → UNEXPECTED, exit 1, no double-throw", async () => {
  const { events, trackCliEvent } = captureEvents();
  let written = null;
  const code = await runCommand({
    name: "kaboom",
    handler: async () => { throw new Error("kaboom"); },
    ctx: {},
    deps: { trackCliEvent, writeError: (info) => { written = info; } },
  });
  assert.equal(code, 1);
  assert.equal(written.code, "UNEXPECTED");
  assert.equal(written.message, "kaboom");
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
