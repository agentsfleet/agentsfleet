import { describe, expect, it } from "vitest";
import { type LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { applyLiveFrame } from "./fleet-stream-frames";
import type { FleetEvent } from "./fleet-stream-row";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";

// The three tool frames: how a call attaches to its event, progresses, and
// completes, and what happens to a frame whose event is not there. The wire
// carries no start instant, so the reducer stamps the first frame's clock.

const STARTED_AT = 1_000;
const PROGRESS_AT = 1_500;
const COMPLETED_AT = 2_000;
const REPEAT_AT = 3_000;

function started(name: string, eventId = "e1"): LiveFrame {
  return { kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: eventId, name, args_redacted: {} };
}

function progressed(name: string, elapsed: number): LiveFrame {
  return { kind: FRAME_KIND.TOOL_CALL_PROGRESS, event_id: "e1", name, elapsed_ms: elapsed };
}

function completed(name: string, ms: number): LiveFrame {
  return { kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "e1", name, ms };
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

  it("TOOL_CALL_PROGRESS updates the running tool's elapsed time in place", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], started("search_repo"), STARTED_AT);
    out = applyLiveFrame(out, progressed("search_repo", 400), PROGRESS_AT);
    expect(out[0]?.tools).toEqual([{ name: "search_repo", startedAtMs: STARTED_AT, ms: 400, done: false }]);
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
    const badName = { kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: "e1", name: { bad: "name" } } as unknown as LiveFrame;
    expect(applyLiveFrame(seed, badName, STARTED_AT)).toBe(seed);
    const emptyName = { kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: "e1", name: "" } as unknown as LiveFrame;
    expect(applyLiveFrame(seed, emptyName, STARTED_AT)).toBe(seed);

    const open = applyLiveFrame(seed, started("shell"), STARTED_AT);
    const textMs = { kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "e1", name: "shell", ms: "7" } as unknown as LiveFrame;
    expect(applyLiveFrame(open, textMs, COMPLETED_AT)).toBe(open);
    const negative = { kind: FRAME_KIND.TOOL_CALL_PROGRESS, event_id: "e1", name: "shell", elapsed_ms: -1 } as unknown as LiveFrame;
    expect(applyLiveFrame(open, negative, PROGRESS_AT)).toBe(open);

    // A completion with no figure is not malformed: it closes the call and
    // keeps the elapsed a progress frame reported.
    const progressed400 = applyLiveFrame(open, progressed("shell", 400), PROGRESS_AT);
    const bare = { kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "e1", name: "shell" } as unknown as LiveFrame;
    expect(applyLiveFrame(progressed400, bare, COMPLETED_AT)[0]?.tools).toEqual([
      { name: "shell", startedAtMs: STARTED_AT, ms: 400, done: true },
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
  });

  // event_received always precedes its tool frames on the wire. Synthesizing an
  // event here would put a message in the thread that the backfill then duplicates.
  it("drops a tool frame whose event has not arrived, rather than inventing an event", () => {
    const seed = [evt({ id: "e1" })];
    expect(applyLiveFrame(seed, started("search_repo", "ghost"), STARTED_AT)).toBe(seed);
  });
});
