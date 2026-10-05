import { describe, expect, it } from "vitest";

import { rowToEvent } from "./fleet-stream-row";
import { TOOL_CALL_STATUS, readSavedTrace, readToolArgs, readToolOutcome } from "./fleet-stream-tool-trace";
import { MS_PER_SECOND, row } from "@/tests/helpers/fleet-stream-fixtures";

// An event's saved trace, narrowed where it enters the browser: the rows a
// reload shows, and what a corrupt body leaves behind.

const READ = "file_read";
const REQUEST = "http_request";
const CALL_ONE = "7:1";
const CALL_TWO = "7:2";
const PATH_A = "a.md";
const READ_MS = 12;
const REQUEST_MS = 1_200;

describe("readSavedTrace", () => {
  it("test_saved_trace_becomes_tool_calls", () => {
    const trace = {
      calls: [
        { call_id: CALL_ONE, name: READ, arguments: { path: PATH_A }, status: TOOL_CALL_STATUS.SUCCEEDED,
          output_head: "# agentsfleet", output_tail: "MIT", output_line_count: 214, duration_ms: READ_MS },
        { call_id: CALL_TWO, name: REQUEST, arguments: { method: "POST", url: "https://example.test/deploy" },
          status: TOOL_CALL_STATUS.FAILED, output_head: "HTTP 403", output_line_count: 1, exit_code: 1, duration_ms: REQUEST_MS },
      ],
      omitted_call_count: 0,
    };
    const event = rowToEvent(row({ tool_calls: trace }));
    // Every call is finished and starts at the event's own instant: only its
    // duration renders.
    expect(event.tools).toEqual([
      { name: READ, callId: CALL_ONE, startedAtMs: MS_PER_SECOND, ms: READ_MS, done: true, args: { path: PATH_A },
        status: TOOL_CALL_STATUS.SUCCEEDED, outputHead: "# agentsfleet", outputTail: "MIT", outputLineCount: 214 },
      { name: REQUEST, callId: CALL_TWO, startedAtMs: MS_PER_SECOND, ms: REQUEST_MS, done: true,
        args: { method: "POST", url: "https://example.test/deploy" },
        status: TOOL_CALL_STATUS.FAILED, outputHead: "HTTP 403", outputLineCount: 1, exitCode: 1 },
    ]);
    expect(event.omittedCallCount).toBe(0);
  });

  it("test_malformed_saved_trace_reads_absent", () => {
    // A body that is not a trace leaves the row without calls, and never throws.
    for (const tool_calls of ["x", 42, [], {}, { calls: "x" }, null, undefined]) {
      const event = rowToEvent(row({ tool_calls: tool_calls as never }));
      expect(event).not.toHaveProperty("tools");
      expect(event).not.toHaveProperty("omittedCallCount");
    }
  });

  it("should count a call too malformed to render with the ones the runner left out", () => {
    const good = { call_id: CALL_ONE, name: READ, arguments: {}, status: TOOL_CALL_STATUS.SUCCEEDED, duration_ms: READ_MS };
    const malformed = [
      { ...good, name: "" },
      { ...good, call_id: 7 },
      { ...good, duration_ms: -1 },
      { ...good, duration_ms: "12" },
      "not a call",
    ];
    const trace = readSavedTrace({ calls: [good, ...malformed], omitted_call_count: 3 }, MS_PER_SECOND);
    expect(trace?.calls.map((call) => call.callId)).toEqual([CALL_ONE]);
    expect(trace?.omitted).toBe(3 + malformed.length);
  });

  it("should keep a saved call whose own fields are malformed, without them", () => {
    const call = {
      call_id: CALL_ONE, name: READ, arguments: ["path"], status: "ok", output_head: 7, output_line_count: -2, duration_ms: READ_MS,
    };
    const trace = readSavedTrace({ calls: [call], omitted_call_count: -1 }, MS_PER_SECOND);
    expect(trace).toEqual({ calls: [{ name: READ, callId: CALL_ONE, startedAtMs: MS_PER_SECOND, ms: READ_MS, done: true }], omitted: 0 });
  });
});

describe("readToolArgs", () => {
  it("should keep an object of JSON values and nothing else", () => {
    const nested = { path: PATH_A, headers: { accept: "text/plain" }, lines: [1, 2], dry: false, limit: null };
    expect(readToolArgs(nested)).toEqual(nested);
    for (const value of [{}, [], [{ path: PATH_A }], "a", 1, true, null, undefined]) {
      expect(readToolArgs(value)).toBeUndefined();
    }
  });
});

describe("readToolOutcome", () => {
  it("should read anything but an object as no outcome", () => {
    for (const value of [[{ status: TOOL_CALL_STATUS.SUCCEEDED }], "succeeded", 0, null, undefined]) {
      expect(readToolOutcome(value)).toEqual({});
    }
  });
});
