// Covers runCommand wrapper. Pins: started/finished event order,
// errorMap dispatch, propagation of retry config to ctx.retryConfig,
// fetch-failed → API_UNREACHABLE, unknown → UNEXPECTED, exit-code
// mapping.

import { test } from "bun:test";
import assert from "node:assert/strict";
import { ApiError } from "../src/lib/http.ts";
import {
  runCommand,
  runCommandInternals,
  type HandlerCtx,
  type RunCommandOptions,
} from "../src/lib/run-command.ts";

interface CapturedEvent { event: string; props: Record<string, unknown> }

function captureEvents() {
  const events: CapturedEvent[] = [];
  const trackCliEvent = (
    _client: unknown,
    _id: string | null | undefined,
    event: string,
    props?: Record<string, unknown>,
  ) => events.push({ event, props: props ?? {} });
  return { events, trackCliEvent };
}

function captureWriter() {
  const lines: string[] = [];
  const jsonPayloads: unknown[] = [];
  const writeLine = (_stream: NodeJS.WritableStream, line = "") => { lines.push(line); };
  const printJson = (_stream: NodeJS.WritableStream, value: unknown) => { jsonPayloads.push(value); };
  const ui = { err: (s: string) => s };
  return { lines, jsonPayloads, writeLine, printJson, ui };
}

// STDERR stub — runCommand only writes to .write() through deps;
// the stub satisfies the WritableStream surface used by the wrapper.
const STDERR_STUB = { write: () => true } as unknown as NodeJS.WritableStream;

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
  assert.equal(events[0]?.props.command, "test-cmd");
  assert.equal(events[1]?.props.exit_code, "0");
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
  assert.equal(events.at(-1)?.event, "cli_command_finished");
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
  assert.ok(last);
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

test("runCommand: errorMap remaps message in render and code in analytics; server UZ-* stays in stderr", async () => {
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
  // Stderr keeps the server code (UZ-*) for support workflows; the
  // message is the friendly remapped text.
  assert.equal(w.lines[0], "error: UZ-VAL-001 Pick a different name.");
  // Analytics records the friendly bucket so dashboards aggregate
  // across server code renames.
  assert.equal(events.at(-1)?.props.error_code, "WORKSPACE_NAME_INVALID");
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
  assert.match(w.lines[0] ?? "", /^error: cannot reach usezombie API at http:\/\/api\.example\.com/);
  assert.equal(events.at(-1)?.props.error_code, "API_UNREACHABLE");
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
  assert.equal(w.lines[0], "error: kaboom");
  assert.equal(events.at(-1)?.props.error_code, "UNEXPECTED");
});

test("runCommand: retry=false sets ctx.retryConfig.maxAttempts=1 (propagation)", async () => {
  let observed: HandlerCtx["retryConfig"] | undefined;
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
  let observed: HandlerCtx["retryConfig"] | undefined;
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
  let observed: HandlerCtx["retryConfig"] | "untouched" = "untouched";
  await runCommand({
    name: "obs",
    handler: async (ctx) => { observed = ctx.retryConfig; return 0; },
    ctx: {},
    deps: { trackCliEvent: () => {} },
  });
  assert.equal(observed, null);
});

test("runCommand: retry=true is treated like undefined (defer to default)", async () => {
  let observed: HandlerCtx["retryConfig"] | "untouched" = "untouched";
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
  // These intentionally violate the RunCommandOptions contract to pin
  // runtime validation — the cast widens to bypass compile-time refusal.
  await assert.rejects(
    () => runCommand({ name: "x" } as unknown as RunCommandOptions),
    TypeError,
  );
  await assert.rejects(
    () => runCommand({ handler: async () => 0 } as unknown as RunCommandOptions),
    TypeError,
  );
  await assert.rejects(
    () => runCommand({ name: "", handler: async () => 0 }),
    TypeError,
  );
});

test("resolveRetryConfig: covers all four input shapes", () => {
  assert.equal(runCommandInternals.resolveRetryConfig(undefined), null);
  assert.equal(runCommandInternals.resolveRetryConfig(true), null);
  assert.deepEqual(runCommandInternals.resolveRetryConfig(false), { maxAttempts: 1 });
  assert.deepEqual(runCommandInternals.resolveRetryConfig({ maxAttempts: 7 }), { maxAttempts: 7 });
});
