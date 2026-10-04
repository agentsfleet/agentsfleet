import { describe, expect, it } from "vitest";
import { type LiveFrame, type SavedToolCall } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { applyLiveFrame, mergeBackfill, parseLiveFrame } from "./fleet-stream-frames";
import type { FleetEvent, FleetToolCall } from "./fleet-stream-row";
import { TOOL_CALL_STATUS } from "./fleet-stream-tool-trace";
import { evt, row } from "@/tests/helpers/fleet-stream-fixtures";

// The three tool frames: how a call attaches to its event, progresses, and
// completes, and what happens to a frame whose event is not there. The wire
// carries no start instant, so the reducer stamps the first frame's clock.

const STARTED_AT = 1_000;
const PROGRESS_AT = 1_500;
const COMPLETED_AT = 2_000;
const REPEAT_AT = 3_000;

// Each builder takes an optional `call_id`, as a runner that names its calls sends.
function started(name: string, eventId = "e1", call_id?: string): LiveFrame {
  return { kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: eventId, name, args_redacted: {}, call_id };
}

function progressed(name: string, elapsed: number, call_id?: string): LiveFrame {
  return { kind: FRAME_KIND.TOOL_CALL_PROGRESS, event_id: "e1", name, elapsed_ms: elapsed, call_id };
}

function completed(name: string, ms: number, call_id?: string): LiveFrame {
  return { kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "e1", name, ms, call_id };
}

// Hostile wire input, through the real parser: it checks only `kind`, which is
// why the reducer reads every other field itself.
function wire(payload: Record<string, unknown>): LiveFrame {
  const frame = parseLiveFrame(JSON.stringify(payload));
  if (frame === null) throw new Error("the parser refused a frame it should let through");
  return frame;
}

describe("applyLiveFrame — tool frames", () => {
  it("a tool frame for an event not yet in the timeline is dropped (same reference back)", () => {
    const seed: FleetEvent[] = [];
    expect(applyLiveFrame(seed, started("shell"), STARTED_AT)).toBe(seed);
  });

  it("TOOL_CALL_STARTED attaches the tool to its event instead of dropping the frame", () => {
    const out = applyLiveFrame([evt({ id: "e1" })], started("search_repo"), STARTED_AT);
    expect(out[0]?.tools).toEqual([{ name: "search_repo", startedAtMs: STARTED_AT, ms: null, done: false }]);
  });

  it("test_progress_frame_keeps_identity", () => {
    const running = applyLiveFrame([evt({ id: "e1" })], started("search_repo"), STARTED_AT);
    expect(applyLiveFrame(running, progressed("search_repo", 400), PROGRESS_AT)).toBe(running);
    const named = applyLiveFrame([evt({ id: "e1" })], started("search_repo", "e1", "1"), STARTED_AT);
    expect(applyLiveFrame(named, progressed("search_repo", 400, "1"), PROGRESS_AT)).toBe(named);
    // Only the completion changes the timeline.
    const done = applyLiveFrame(named, completed("search_repo", 900, "1"), COMPLETED_AT);
    expect(done).not.toBe(named);
    expect(done[0]?.tools).toEqual([{ name: "search_repo", callId: "1", startedAtMs: STARTED_AT, ms: 900, done: true }]);
  });

  it("test_tool_frames_pair_by_call_id", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], started("read_file", "e1", "1"), STARTED_AT);
    out = applyLiveFrame(out, completed("read_file", 700, "1"), COMPLETED_AT);
    // A second call's completion, its start missed, restating the first one's
    // figure: timing would call it a repeat; its own id makes it a new call.
    out = applyLiveFrame(out, completed("read_file", 700, "2"), REPEAT_AT);
    expect(out[0]?.tools).toEqual([
      { name: "read_file", callId: "1", startedAtMs: STARTED_AT, ms: 700, done: true },
      { name: "read_file", callId: "2", startedAtMs: REPEAT_AT, ms: 700, done: true },
    ]);
    // A late frame for a call that finished changes nothing.
    expect(applyLiveFrame(out, progressed("read_file", 100, "1"), REPEAT_AT)).toBe(out);
    expect(applyLiveFrame(out, started("read_file", "e1", "2"), REPEAT_AT)).toBe(out);
  });

  it("pairs a frame whose call id is not a non-empty string by timing, as before", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], started("grep"), STARTED_AT);
    out = applyLiveFrame(out, completed("grep", 300, ""), COMPLETED_AT);
    out = applyLiveFrame(out, wire({ kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "e1", name: "grep", ms: 300, call_id: 7 }), REPEAT_AT);
    expect(out[0]?.tools).toEqual([{ name: "grep", startedAtMs: STARTED_AT, ms: 300, done: true }]);
  });

  it("TOOL_CALL_COMPLETED marks the tool done with its final wall time", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], started("search_repo"), STARTED_AT);
    out = applyLiveFrame(out, completed("search_repo", 1_200), COMPLETED_AT);
    expect(out[0]?.tools).toEqual([{ name: "search_repo", startedAtMs: STARTED_AT, ms: 1_200, done: true }]);
  });

  it("test_tool_start_stamped_from_first_frame", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], started("grep"), STARTED_AT);
    out = applyLiveFrame(out, progressed("grep", 500), PROGRESS_AT);
    out = applyLiveFrame(out, completed("grep", 700), COMPLETED_AT);
    // The same tool called again after it finished is its own call, with its
    // own start.
    out = applyLiveFrame(out, started("grep"), REPEAT_AT);
    expect(out[0]?.tools).toEqual([
      { name: "grep", startedAtMs: STARTED_AT, ms: 700, done: true },
      { name: "grep", startedAtMs: REPEAT_AT, ms: null, done: false },
    ]);
  });

  it("a completion first seen without a start still stamps its arrival", () => {
    const out = applyLiveFrame([evt({ id: "e1" })], completed("late", 250), COMPLETED_AT);
    expect(out[0]?.tools).toEqual([{ name: "late", startedAtMs: COMPLETED_AT, ms: 250, done: true }]);
  });

  it("keeps two distinct tools on one event, in first-seen order", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], started("first"), STARTED_AT);
    out = applyLiveFrame(out, started("second"), PROGRESS_AT);
    expect(out[0]?.tools?.map((t) => t.name)).toEqual(["first", "second"]);
  });

  it("updating one tool leaves a coexisting tool untouched (bystander arm)", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], started("first"), STARTED_AT);
    out = applyLiveFrame(out, started("second"), PROGRESS_AT);
    out = applyLiveFrame(out, completed("second", 250), COMPLETED_AT);
    expect(out[0]?.tools).toEqual([
      { name: "first", startedAtMs: STARTED_AT, ms: null, done: false },
      { name: "second", startedAtMs: PROGRESS_AT, ms: 250, done: true },
    ]);
  });

  it("test_malformed_tool_frame_dropped", () => {
    // `parseLiveFrame` checks only `kind`; the tool row renders `name` as a
    // React child, so an object there would throw on render.
    const seed = [evt({ id: "e1" })];
    const badName = wire({ kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: "e1", name: { bad: "name" } });
    expect(applyLiveFrame(seed, badName, STARTED_AT)).toBe(seed);
    const emptyName = wire({ kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: "e1", name: "" });
    expect(applyLiveFrame(seed, emptyName, STARTED_AT)).toBe(seed);

    const open = applyLiveFrame(seed, started("shell"), STARTED_AT);
    const textMs = wire({ kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "e1", name: "shell", ms: "7" });
    expect(applyLiveFrame(open, textMs, COMPLETED_AT)).toBe(open);
    const negative = wire({ kind: FRAME_KIND.TOOL_CALL_PROGRESS, event_id: "e1", name: "shell", elapsed_ms: -1 });
    expect(applyLiveFrame(open, negative, PROGRESS_AT)).toBe(open);

    // A completion with no figure is not malformed: it closes the call and
    // keeps the elapsed the call holds — here from the progress frame that
    // opened it, its start missed.
    const progressed400 = applyLiveFrame(seed, progressed("shell", 400), PROGRESS_AT);
    const bare = wire({ kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "e1", name: "shell" });
    expect(applyLiveFrame(progressed400, bare, COMPLETED_AT)[0]?.tools).toEqual([
      { name: "shell", startedAtMs: PROGRESS_AT, ms: 400, done: true },
    ]);
  });

  it("test_late_tool_frame_opens_nothing", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], started("read_file"), STARTED_AT);
    out = applyLiveFrame(out, completed("read_file", 700), COMPLETED_AT);
    // The same completion again, and a progress frame that arrived after it:
    // neither invents a second call.
    const settled = out;
    expect(applyLiveFrame(settled, completed("read_file", 700), REPEAT_AT)).toBe(settled);
    expect(applyLiveFrame(settled, progressed("read_file", 300), REPEAT_AT)).toBe(settled);
    // A repeated start while the call is open moves nothing either.
    const running = applyLiveFrame([evt({ id: "e1" })], started("grep"), STARTED_AT);
    expect(applyLiveFrame(running, started("grep"), PROGRESS_AT)).toBe(running);
    expect(running[0]?.tools).toHaveLength(1);
    // A progress frame restating the elapsed changes nothing.
    const at400 = applyLiveFrame(running, progressed("grep", 400), PROGRESS_AT);
    expect(applyLiveFrame(at400, progressed("grep", 400), COMPLETED_AT)).toBe(at400);
    // A completion with no figure could be the finished call's, and a call that
    // finished without one cannot be told from a new one.
    const bare = wire({ kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "e1", name: "read_file" });
    expect(applyLiveFrame(settled, bare, REPEAT_AT)).toBe(settled);
    const unmeasured = applyLiveFrame(running, wire({ ...bare, name: "grep" }), COMPLETED_AT);
    expect(applyLiveFrame(unmeasured, completed("grep", 900), REPEAT_AT)).toBe(unmeasured);
  });

  it("shows a second call whose start a reconnect missed, once its timing proves it new", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], started("read_file"), STARTED_AT);
    out = applyLiveFrame(out, completed("read_file", 700), COMPLETED_AT);
    const first = { name: "read_file", startedAtMs: STARTED_AT, ms: 700, done: true };
    // Progress past where the first call ended is a second call...
    expect(applyLiveFrame(out, progressed("read_file", 900), REPEAT_AT)[0]?.tools).toEqual([
      first,
      { name: "read_file", startedAtMs: REPEAT_AT, ms: 900, done: false },
    ]);
    // ...and so is a completion with a figure of its own.
    expect(applyLiveFrame(out, completed("read_file", 250), REPEAT_AT)[0]?.tools).toEqual([
      first,
      { name: "read_file", startedAtMs: REPEAT_AT, ms: 250, done: true },
    ]);
  });

  // event_received always precedes its tool frames on the wire. Synthesizing an
  // event here would put a message in the thread that the backfill then duplicates.
  it("drops a tool frame whose event has not arrived, rather than inventing an event", () => {
    const seed = [evt({ id: "e1" })];
    expect(applyLiveFrame(seed, started("search_repo", "ghost"), STARTED_AT)).toBe(seed);
  });
});

const READ = "file_read";
const REQUEST = "http_request";
const CALL_ONE = "7:1";
const CALL_TWO = "7:2";
const CALL_THREE = "7:3";
const PATH_A = "a.md";
const REQUEST_MS = 400;
const DENIED = "denied";

function startedWith(name: string, args: unknown, call_id?: string): LiveFrame {
  return wire({ kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: "e1", name, args_redacted: args, call_id });
}

function completedWith(name: string, ms: number, outcome: Record<string, unknown>, call_id?: string): LiveFrame {
  return wire({ kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "e1", name, ms, call_id, ...outcome });
}

describe("applyLiveFrame — tool arguments and outcome", () => {
  it("test_started_frame_keeps_arguments", () => {
    const out = applyLiveFrame([evt({ id: "e1" })], startedWith(READ, { path: PATH_A }, CALL_ONE), STARTED_AT);
    expect(out[0]?.tools).toEqual([
      { name: READ, callId: CALL_ONE, startedAtMs: STARTED_AT, ms: null, done: false, args: { path: PATH_A } },
    ]);
    // `{}` names no arguments, so the call carries none rather than an empty bag.
    const bare = applyLiveFrame([evt({ id: "e1" })], startedWith(READ, {}, CALL_ONE), STARTED_AT);
    expect(bare[0]?.tools?.[0]).not.toHaveProperty("args");
  });

  it("test_repeat_start_merges_arguments_keeps_clock", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], startedWith(READ, {}, CALL_ONE), STARTED_AT);
    out = applyLiveFrame(out, startedWith(READ, { path: PATH_A }, CALL_ONE), REPEAT_AT);
    expect(out[0]?.tools).toEqual([
      { name: READ, callId: CALL_ONE, startedAtMs: STARTED_AT, ms: null, done: false, args: { path: PATH_A } },
    ]);
    // A later repeat adds to what the call holds; it never drops a key.
    out = applyLiveFrame(out, startedWith(READ, { limit: 5 }, CALL_ONE), REPEAT_AT);
    expect(out[0]?.tools?.[0]?.args).toEqual({ path: PATH_A, limit: 5 });
    // One that adds nothing returns the timeline unchanged.
    expect(applyLiveFrame(out, startedWith(READ, { path: PATH_A }, CALL_ONE), REPEAT_AT)).toBe(out);
    expect(applyLiveFrame(out, startedWith(READ, {}, CALL_ONE), REPEAT_AT)).toBe(out);

    // A runner that names no call merges into the open call of that name.
    let timed = applyLiveFrame([evt({ id: "e1" })], started("grep"), STARTED_AT);
    timed = applyLiveFrame(timed, startedWith("grep", { pattern: "deploy" }), REPEAT_AT);
    expect(timed[0]?.tools).toEqual([{ name: "grep", startedAtMs: STARTED_AT, ms: null, done: false, args: { pattern: "deploy" } }]);
  });

  it("test_completed_frame_keeps_outcome", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], startedWith(REQUEST, { method: "POST" }, CALL_TWO), STARTED_AT);
    const outcome = { status: TOOL_CALL_STATUS.FAILED, output_head: DENIED, output_tail: DENIED, output_line_count: 1, exit_code: 2 };
    out = applyLiveFrame(out, completedWith(REQUEST, REQUEST_MS, outcome, CALL_TWO), COMPLETED_AT);
    expect(out[0]?.tools).toEqual([{
      name: REQUEST, callId: CALL_TWO, startedAtMs: STARTED_AT, ms: REQUEST_MS, done: true, args: { method: "POST" },
      status: TOOL_CALL_STATUS.FAILED, outputHead: DENIED, outputTail: DENIED, outputLineCount: 1, exitCode: 2,
    }]);
    // Paired by timing, and first seen at its completion, the outcome lands the same way.
    const byTiming = applyLiveFrame(
      applyLiveFrame([evt({ id: "e1" })], started(REQUEST), STARTED_AT),
      completedWith(REQUEST, REQUEST_MS, { status: TOOL_CALL_STATUS.SUCCEEDED }),
      COMPLETED_AT,
    );
    expect(byTiming[0]?.tools?.[0]).toMatchObject({ done: true, status: TOOL_CALL_STATUS.SUCCEEDED });
    const firstSeen = applyLiveFrame([evt({ id: "e1" })], completedWith(REQUEST, REQUEST_MS, { output_line_count: 0 }, CALL_THREE), COMPLETED_AT);
    expect(firstSeen[0]?.tools).toEqual([{ name: REQUEST, callId: CALL_THREE, startedAtMs: COMPLETED_AT, ms: REQUEST_MS, done: true, outputLineCount: 0 }]);
  });

  it("test_malformed_tool_fields_read_absent", () => {
    const seed = [evt({ id: "e1" })];
    // Arguments that are not an object of JSON values name nothing.
    for (const args of [[1], "path", 7, null, undefined]) {
      expect(applyLiveFrame(seed, startedWith(READ, args), STARTED_AT)[0]?.tools).toEqual([
        { name: READ, startedAtMs: STARTED_AT, ms: null, done: false },
      ]);
    }
    const open = applyLiveFrame(seed, started(READ), STARTED_AT);
    const malformed = { status: "ok", output_head: 7, output_tail: ["tail"], output_line_count: -1, exit_code: 1.5 };
    expect(applyLiveFrame(open, completedWith(READ, REQUEST_MS, malformed), COMPLETED_AT)[0]?.tools).toEqual([
      { name: READ, startedAtMs: STARTED_AT, ms: REQUEST_MS, done: true },
    ]);
    // One bad field leaves the good ones standing.
    const mixed = { status: TOOL_CALL_STATUS.SUCCEEDED, output_head: "# readme", output_line_count: 2.5 };
    expect(applyLiveFrame(open, completedWith(READ, REQUEST_MS, mixed), COMPLETED_AT)[0]?.tools).toEqual([
      { name: READ, startedAtMs: STARTED_AT, ms: REQUEST_MS, done: true, status: TOOL_CALL_STATUS.SUCCEEDED, outputHead: "# readme" },
    ]);
  });
});

// A saved call as `GET …/messages` serves it.
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

  it("test_settle_interrupts_open_live_calls", () => {
    const live = [evt({ id: "e1", status: "received", tools: [openCall({ ms: 300 }), openCall({ callId: CALL_TWO, ms: 9, done: true })] })];
    const settled = mergeBackfill(live, [row({ event_id: "e1", tool_calls: null })]);
    expect(settled[0]?.tools).toEqual([
      openCall({ ms: 300, done: true, status: TOOL_CALL_STATUS.INTERRUPTED }),
      openCall({ callId: CALL_TWO, ms: 9, done: true }),
    ]);
    // The completion frame ends the turn the same way, and a completion that
    // arrives after it changes nothing.
    const completed = applyLiveFrame(live, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "e1", status: "processed" }, COMPLETED_AT);
    expect(completed[0]?.tools?.[0]).toEqual(openCall({ ms: 300, done: true, status: TOOL_CALL_STATUS.INTERRUPTED }));
    expect(applyLiveFrame(completed, completedWith(READ, REQUEST_MS, {}, CALL_ONE), REPEAT_AT)).toBe(completed);
    // A turn with no open call keeps its calls as they were.
    const quiet = [evt({ id: "e1", status: "received", tools: [openCall({ done: true })] })];
    const ended = applyLiveFrame(quiet, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "e1", status: "processed" }, COMPLETED_AT);
    expect(ended[0]?.tools).toBe(quiet[0]?.tools);
  });
});
