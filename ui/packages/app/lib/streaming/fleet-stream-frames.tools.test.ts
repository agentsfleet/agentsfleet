import { describe, expect, it } from "vitest";
import { FRAME_KIND, type LiveFrame } from "@/lib/api/events";
import { applyLiveFrame } from "./fleet-stream-frames";
import type { FleetEvent } from "./fleet-stream-row";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";

// The three tool frames: how a call attaches to its event, progresses, and
// completes, and what happens to a frame whose event is not there.

describe("applyLiveFrame — tool frames", () => {
  it("a tool frame for an event not yet in the timeline is dropped (same reference back)", () => {
    const seed: FleetEvent[] = [];
    const frame: LiveFrame = {
      kind: FRAME_KIND.TOOL_CALL_STARTED,
      event_id: "e1",
      name: "shell",
      args_redacted: {},
    };
    expect(applyLiveFrame(seed, frame)).toBe(seed);
  });

  it("TOOL_CALL_STARTED attaches the tool to its event instead of dropping the frame", () => {
    const seed = [evt({ id: "e1" })];
    const out = applyLiveFrame(seed, {
      kind: FRAME_KIND.TOOL_CALL_STARTED,
      event_id: "e1",
      name: "search_repo",
      args_redacted: {},
    });
    expect(out[0]?.tools).toEqual([{ name: "search_repo", ms: null, done: false }]);
  });

  it("TOOL_CALL_PROGRESS updates the running tool's elapsed time in place", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], {
      kind: FRAME_KIND.TOOL_CALL_STARTED,
      event_id: "e1",
      name: "search_repo",
      args_redacted: {},
    });
    out = applyLiveFrame(out, {
      kind: FRAME_KIND.TOOL_CALL_PROGRESS,
      event_id: "e1",
      name: "search_repo",
      elapsed_ms: 400,
    });
    expect(out[0]?.tools).toEqual([{ name: "search_repo", ms: 400, done: false }]);
  });

  it("TOOL_CALL_COMPLETED marks the tool done with its final wall time", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], {
      kind: FRAME_KIND.TOOL_CALL_STARTED,
      event_id: "e1",
      name: "search_repo",
      args_redacted: {},
    });
    out = applyLiveFrame(out, {
      kind: FRAME_KIND.TOOL_CALL_COMPLETED,
      event_id: "e1",
      name: "search_repo",
      ms: 1_200,
    });
    expect(out[0]?.tools).toEqual([{ name: "search_repo", ms: 1_200, done: true }]);
  });

  it("keeps two distinct tools on one event, in first-seen order", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], {
      kind: FRAME_KIND.TOOL_CALL_STARTED,
      event_id: "e1",
      name: "first",
      args_redacted: {},
    });
    out = applyLiveFrame(out, {
      kind: FRAME_KIND.TOOL_CALL_STARTED,
      event_id: "e1",
      name: "second",
      args_redacted: {},
    });
    expect(out[0]?.tools?.map((t) => t.name)).toEqual(["first", "second"]);
  });

  // The same tool can be called twice in one event. The second call must not
  // reopen the finished first one.
  it("a second call to the same tool starts a new entry rather than reviving the finished one", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], {
      kind: FRAME_KIND.TOOL_CALL_STARTED,
      event_id: "e1",
      name: "grep",
      args_redacted: {},
    });
    out = applyLiveFrame(out, {
      kind: FRAME_KIND.TOOL_CALL_COMPLETED,
      event_id: "e1",
      name: "grep",
      ms: 90,
    });
    out = applyLiveFrame(out, {
      kind: FRAME_KIND.TOOL_CALL_STARTED,
      event_id: "e1",
      name: "grep",
      args_redacted: {},
    });
    expect(out[0]?.tools).toEqual([
      { name: "grep", ms: 90, done: true },
      { name: "grep", ms: null, done: false },
    ]);
  });

  it("updating one tool leaves a coexisting tool untouched (bystander arm)", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], {
      kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: "e1", name: "first", args_redacted: {},
    });
    out = applyLiveFrame(out, {
      kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: "e1", name: "second", args_redacted: {},
    });
    out = applyLiveFrame(out, {
      kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "e1", name: "second", ms: 250,
    });
    expect(out[0]?.tools).toEqual([
      { name: "first", ms: null, done: false },
      { name: "second", ms: 250, done: true },
    ]);
  });

  // event_received always precedes its tool frames on the wire. Synthesizing an
  // event here would put a message in the thread that the backfill then duplicates.
  it("drops a tool frame whose event has not arrived, rather than inventing an event", () => {
    const seed = [evt({ id: "e1" })];
    const out = applyLiveFrame(seed, {
      kind: FRAME_KIND.TOOL_CALL_STARTED,
      event_id: "ghost",
      name: "search_repo",
      args_redacted: {},
    });
    expect(out).toBe(seed);
  });
});
