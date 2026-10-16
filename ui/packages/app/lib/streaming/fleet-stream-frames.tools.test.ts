import { describe, expect, it } from "vitest";
import { type LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { applyLiveFrame, parseLiveFrame } from "./fleet-stream-frames";
import { TOOL_CALL_STATUS } from "./fleet-stream-tool-trace";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";

const FIRST = "7:1";
const SECOND = "7:2";
const START = 1000;
const END = 2000;
const NAME = "file_read";
const SUCCESS = TOOL_CALL_STATUS.SUCCEEDED;
function started(call_id = FIRST, args_redacted: unknown = {}): LiveFrame {
  return {kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: "e1", name: NAME, call_id, args_redacted};
}
function completed(call_id = FIRST, ms = 700): LiveFrame {
  return {kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "e1", name: NAME, call_id, ms, status: SUCCESS};
}
function wire(payload: Record<string, unknown>): LiveFrame {
  const frame = parseLiveFrame(JSON.stringify(payload));
  if (frame === null) throw new Error("Expected a JSON frame");
  return frame;
}
const seed = () => [evt({id: "e1"})];
describe("tool frames with required identifiers and outcomes", () => {
  it("opens a pending call and completes the same identifier", () => {
    const open = applyLiveFrame(seed(), started(), START);
    expect(open[0]?.tools).toEqual([{name: NAME, callId: FIRST, startedAtMs: START, ms: null, done: false}]);
    const done = applyLiveFrame(open, completed(), END);
    expect(done[0]?.tools).toEqual([{name: NAME, callId: FIRST, startedAtMs: START, ms: 700, done: true, status: SUCCESS}]);
  });
  it("keeps a progress update on an open call referentially unchanged", () => {
    const open = applyLiveFrame(seed(), started(), START);
    expect(applyLiveFrame(open, {kind: FRAME_KIND.TOOL_CALL_PROGRESS, event_id: "e1", name: NAME, call_id: FIRST, elapsed_ms: 400}, END)).toBe(open);
  });
  it("uses identifiers to separate same-name calls even with equal timings", () => {
    const done = applyLiveFrame(applyLiveFrame(seed(), started(), START), completed(), END);
    const two = applyLiveFrame(done, completed(SECOND), END);
    expect(two[0]?.tools?.map((call) => [call.callId, call.ms])).toEqual([[FIRST, 700], [SECOND, 700]]);
    expect(applyLiveFrame(two, completed(), END)).toBe(two);
    expect(applyLiveFrame(two, started(SECOND), END)).toBe(two);
  });
  it("stamps a missed start from the first observed completion", () => {
    expect(applyLiveFrame(seed(), completed(), END)[0]?.tools?.[0]).toEqual({name: NAME, callId: FIRST, startedAtMs: END, ms: 700, done: true, status: SUCCESS});
  });
  it("merges repeated start arguments without resetting the clock", () => {
    const open = applyLiveFrame(seed(), started(FIRST, {path: "a.md"}), START);
    const merged = applyLiveFrame(open, started(FIRST, {limit: 5}), END);
    expect(merged[0]?.tools?.[0]).toEqual({name: NAME, callId: FIRST, startedAtMs: START, ms: null, done: false, args: {path: "a.md", limit: 5}});
    expect(applyLiveFrame(merged, started(FIRST, {path: "a.md"}), END)).toBe(merged);
    expect(applyLiveFrame(merged, started(), END)).toBe(merged);
  });
  it("leaves the other call unchanged when one completes", () => {
    const open = applyLiveFrame(applyLiveFrame(seed(), started(), START), started(SECOND), END);
    const done = applyLiveFrame(open, completed(SECOND), END);
    expect(done[0]?.tools?.[0]).toBe(open[0]?.tools?.[0]);
    expect(done[0]?.tools?.[1]).toMatchObject({callId: SECOND, done: true, status: SUCCESS});
  });
  it("drops a tool frame without an admitted event", () => {
    const empty: ReturnType<typeof seed> = [];
    expect(applyLiveFrame(empty, started(), START)).toBe(empty);
  });
  it.each(["", {}, 7, null])("drops a malformed tool name %s", (name) => {
    const prev = seed();
    expect(applyLiveFrame(prev, wire({...started(), name}), START)).toBe(prev);
  });
  it.each([undefined, null, -1, "7", Number.POSITIVE_INFINITY])("drops malformed completion timing %s", (ms) => {
    const open = applyLiveFrame(seed(), started(), START);
    expect(applyLiveFrame(open, wire({...completed(), ms}), END)).toBe(open);
  });
  it.each([[], "path", 7, null, undefined])("omits malformed arguments %s", (args) => {
    expect(applyLiveFrame(seed(), started(FIRST, args), START)[0]?.tools?.[0]).not.toHaveProperty("args");
  });
  it("keeps output edges and a nonzero exit code from a failed call", () => {
    const frame = {...completed(), status: TOOL_CALL_STATUS.FAILED, output_head: "denied", output_tail: "tail", output_line_count: 10, exit_code: 2};
    expect(applyLiveFrame(seed(), frame, END)[0]?.tools?.[0]).toMatchObject({status: TOOL_CALL_STATUS.FAILED, outputHead: "denied", outputTail: "tail", outputLineCount: 10, exitCode: 2});
  });
  it("keeps status when optional output fields are malformed", () => {
    const frame = wire({...completed(), output_head: 7, output_tail: [], output_line_count: -1, exit_code: 1.5});
    expect(applyLiveFrame(seed(), frame, END)[0]?.tools?.[0]).toEqual({name: NAME, callId: FIRST, startedAtMs: END, ms: 700, done: true, status: SUCCESS});
  });
});
