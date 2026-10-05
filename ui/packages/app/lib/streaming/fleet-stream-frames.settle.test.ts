import { describe, expect, it } from "vitest";
import { type LiveFrame, type SavedToolCall } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { applyLiveFrame, mergeBackfill, parseLiveFrame } from "./fleet-stream-frames";
import type { FleetToolCall } from "./fleet-stream-row";
import { TOOL_CALL_STATUS } from "./fleet-stream-tool-trace";
import { evt, row } from "@/tests/helpers/fleet-stream-fixtures";

// How a turn's calls settle: the saved trace replaces the live list, open
// calls close as a guess, and a completion that lands later still replaces
// it. Split from fleet-stream-frames.tools.test.ts at its length cap.

const STARTED_AT = 1_000;
const COMPLETED_AT = 2_000;
const REPEAT_AT = 3_000;
const READ = "file_read";
const CALL_ONE = "7:1";
const CALL_TWO = "7:2";
const CALL_THREE = "7:3";
const PATH_A = "a.md";
const REQUEST_MS = 400;

// Hostile wire input, through the real parser: it checks only `kind`.
function completedWith(name: string, ms: number, outcome: Record<string, unknown>, call_id?: string): LiveFrame {
  const frame = parseLiveFrame(JSON.stringify({ kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "e1", name, ms, call_id, ...outcome }));
  if (frame === null) throw new Error("the parser refused a frame it should let through");
  return frame;
}

function saved(call_id: string, over: Partial<SavedToolCall> = {}): SavedToolCall {
  return { call_id, name: READ, arguments: { path: PATH_A }, status: TOOL_CALL_STATUS.SUCCEEDED, duration_ms: 12, ...over };
}

function openCall(over: Partial<FleetToolCall> = {}): FleetToolCall {
  return { name: READ, callId: CALL_ONE, startedAtMs: STARTED_AT, ms: null, done: false, ...over };
}

describe("settling a turn's tool calls", () => {
  it("test_settle_prefers_saved_trace", () => {
    const live = [evt({ id: "e1", status: "received", tools: [openCall()] })];
    const trace = { calls: [saved(CALL_ONE), saved(CALL_TWO), saved(CALL_THREE)], omitted_call_count: 0 };
    const merged = mergeBackfill(live, [row({ event_id: "e1", tool_calls: trace })]);
    expect(merged[0]?.tools?.map((call) => [call.callId, call.done])).toEqual([[CALL_ONE, true], [CALL_TWO, true], [CALL_THREE, true]]);
    // The call both lists hold takes the saved outcome, not the live guess.
    expect(merged[0]?.tools?.[0]).toMatchObject({ callId: CALL_ONE, status: TOOL_CALL_STATUS.SUCCEEDED, ms: 12, args: { path: PATH_A } });
    // A page restating the same trace keeps the settled row's object.
    expect(mergeBackfill(merged, [row({ event_id: "e1", tool_calls: trace })])).toBe(merged);

    // No trace recorded: the live rows stay, finished ones untouched.
    const finished = [evt({ id: "e1", status: "received", tools: [openCall({ ms: 9, done: true })] })];
    const kept = mergeBackfill(finished, [row({ event_id: "e1", tool_calls: null })]);
    expect(kept[0]?.tools).toBe(finished[0]?.tools);
    // A list row carries no trace at all, and keeps them the same way.
    expect(mergeBackfill(finished, [row({ event_id: "e1" })])[0]?.tools).toBe(finished[0]?.tools);
    // A turn shown from its trace keeps the count of calls the trace left out.
    const reloaded = [evt({ id: "e1", status: "received", tools: [openCall({ done: true })], omittedCallCount: 4 })];
    expect(mergeBackfill(reloaded, [row({ event_id: "e1" })])[0]?.omittedCallCount).toBe(4);
  });

  it("test_late_completion_replaces_the_settle_guess", () => {
    // The runner's activity is best-effort and its report may overtake it: the
    // turn ends with the call still open, then the call's own completion lands.
    const live = [evt({ id: "e1", status: "received", tools: [openCall({ ms: 300 })] })];
    const completed = applyLiveFrame(live, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "e1", status: "processed" }, COMPLETED_AT);
    const late = applyLiveFrame(completed, completedWith(READ, REQUEST_MS, { status: TOOL_CALL_STATUS.SUCCEEDED }, CALL_ONE), REPEAT_AT);
    expect(late[0]?.tools?.[0]).toEqual(openCall({ ms: REQUEST_MS, done: true, status: TOOL_CALL_STATUS.SUCCEEDED }));
    // One that names no status still clears the guess rather than keeping it.
    const unsaid = applyLiveFrame(completed, completedWith(READ, REQUEST_MS, {}, CALL_ONE), REPEAT_AT);
    expect(unsaid[0]?.tools?.[0]).toEqual(openCall({ ms: REQUEST_MS, done: true }));
    // Paired by timing when the runner names no call id.
    const anonymous = applyLiveFrame(
      [evt({ id: "e1", status: "received", tools: [openCall({ callId: undefined })] })],
      { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "e1", status: "processed" },
      COMPLETED_AT,
    );
    const paired = applyLiveFrame(anonymous, completedWith(READ, REQUEST_MS, { status: TOOL_CALL_STATUS.FAILED }), REPEAT_AT);
    expect(paired[0]?.tools).toHaveLength(1);
    expect(paired[0]?.tools?.[0]).toMatchObject({ status: TOOL_CALL_STATUS.FAILED, done: true });
    expect(paired[0]?.tools?.[0]?.closedAtSettle).toBeUndefined();
  });

  it("should keep a repeat of a reported call off the call its turn closed", () => {
    // No call ids: the first call reported, the second was still open when the
    // turn ended. Tool frames can repeat, so the first one's completion may land again.
    const first = openCall({ callId: undefined, ms: 100, done: true, status: TOOL_CALL_STATUS.SUCCEEDED });
    const live = [evt({ id: "e1", status: "received", tools: [first, openCall({ callId: undefined })] })];
    const completed = applyLiveFrame(live, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "e1", status: "processed" }, COMPLETED_AT);
    expect(completed[0]?.tools?.[1]).toMatchObject({ status: TOOL_CALL_STATUS.INTERRUPTED, closedAtSettle: true });
    // Its figure restates the first call, so it is the first call's repeat.
    expect(applyLiveFrame(completed, completedWith(READ, 100, { status: TOOL_CALL_STATUS.SUCCEEDED }), REPEAT_AT)).toBe(completed);
    // One past that figure is the second call's own, and lands on it.
    const late = applyLiveFrame(completed, completedWith(READ, REQUEST_MS, { status: TOOL_CALL_STATUS.SUCCEEDED }), REPEAT_AT);
    expect(late[0]?.tools).toEqual([first, openCall({ callId: undefined, ms: REQUEST_MS, done: true, status: TOOL_CALL_STATUS.SUCCEEDED })]);
    // A repeated start lands its arguments on the closed call, opening no other.
    const started = parseLiveFrame(JSON.stringify({ kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: "e1", name: READ, args_redacted: { path: PATH_A } }));
    if (started === null) throw new Error("the parser refused a frame it should let through");
    const restarted = applyLiveFrame(completed, started, REPEAT_AT);
    expect(restarted[0]?.tools).toHaveLength(2);
    expect(restarted[0]?.tools?.[1]).toMatchObject({ args: { path: PATH_A }, closedAtSettle: true });
  });

  it("test_settle_interrupts_open_live_calls", () => {
    const live = [evt({ id: "e1", status: "received", tools: [openCall({ ms: 300 }), openCall({ callId: CALL_TWO, ms: 9, done: true })] })];
    const settled = mergeBackfill(live, [row({ event_id: "e1", tool_calls: null })]);
    expect(settled[0]?.tools).toEqual([
      openCall({ ms: 300, done: true, status: TOOL_CALL_STATUS.INTERRUPTED, closedAtSettle: true }),
      openCall({ callId: CALL_TWO, ms: 9, done: true }),
    ]);
    // The completion frame ends the turn the same way.
    const completed = applyLiveFrame(live, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "e1", status: "processed" }, COMPLETED_AT);
    expect(completed[0]?.tools?.[0]).toEqual(openCall({ ms: 300, done: true, status: TOOL_CALL_STATUS.INTERRUPTED, closedAtSettle: true }));
    // A call it heard end stays as it ended: a repeated completion changes nothing.
    expect(applyLiveFrame(completed, completedWith(READ, REQUEST_MS, {}, CALL_TWO), REPEAT_AT)).toBe(completed);
    // A turn with no open call keeps its calls as they were.
    const quiet = [evt({ id: "e1", status: "received", tools: [openCall({ done: true })] })];
    const ended = applyLiveFrame(quiet, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "e1", status: "processed" }, COMPLETED_AT);
    expect(ended[0]?.tools).toBe(quiet[0]?.tools);
  });
});
