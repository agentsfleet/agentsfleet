import { describe, expect, it } from "vitest";
import { FRAME_KIND, type LiveFrame } from "@/lib/api/events";
import { HEADLINE, OUTCOME } from "@/lib/events/event-summary";
import { applyLiveFrame } from "./fleet-stream-frames";
import type { FleetEvent } from "./fleet-stream-row";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";

// The opening, the chunks and the closing: how the bracket and reply frames
// fold into the timeline, and that each rebuilds only its own row. Tool
// frames are `fleet-stream-frames.tools.test.ts`; the merge is the base suite.

describe("applyLiveFrame", () => {
  it("EVENT_RECEIVED appends a new event then dedupes a repeat by id", () => {
    const frame: LiveFrame = { kind: FRAME_KIND.EVENT_RECEIVED, event_id: "e1", actor: "steer:bob" };
    const once = applyLiveFrame([], frame);
    expect(once).toHaveLength(1);
    expect(once[0]?.role).toBe("user");
    const twice = applyLiveFrame(once, frame);
    expect(twice).toBe(once); // unchanged reference — no duplicate row
  });

  it("EVENT_RECEIVED for a webhook actor renders the neutral floor, not a chat caption", () => {
    // The frame carries no payload and no event type — fabricating "chat"
    // captioned webhook/cron turns as "chat received" until reload.
    const frame: LiveFrame = { kind: FRAME_KIND.EVENT_RECEIVED, event_id: "e2", actor: "webhook:github" };
    const out = applyLiveFrame([], frame);
    expect(out[0]).toMatchObject({ role: "system", text: HEADLINE.EVENT_FALLBACK });
  });

  it("CHUNK creates an assistant event when none exists, accumulating into the reply", () => {
    const created = applyLiveFrame([], { kind: FRAME_KIND.CHUNK, event_id: "e9", text: "Hel" });
    expect(created[0]).toMatchObject({ role: "assistant", actor: "fleet", text: "", reply: "Hel" });
    const appended = applyLiveFrame(created, { kind: FRAME_KIND.CHUNK, event_id: "e9", text: "lo" });
    // The chunk stream is the fleet's reply — it never becomes the trigger text.
    expect(appended[0]?.reply).toBe("Hello");
    expect(appended[0]?.text).toBe("");
  });

  it("CHUNK on an operator turn appends to the reply, never the operator's own text", () => {
    const seed = [evt({ id: "e9", role: "user", actor: "steer:x", text: "Hi ", reply: "" })];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.CHUNK, event_id: "e9", text: "there" });
    expect(out[0]).toMatchObject({ role: "user", text: "Hi ", reply: "there" });
  });

  it("EVENT_COMPLETE sets the reported status", () => {
    const seed = [evt({ id: "e9" })];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "e9", status: "gate_blocked" });
    expect(out[0]?.status).toBe("gate_blocked");
  });

  it("EVENT_COMPLETE carries the failure cause into the outcome live", () => {
    const seed = [evt({ id: "e9" })];
    const out = applyLiveFrame(seed, {
      kind: FRAME_KIND.EVENT_COMPLETE,
      event_id: "e9",
      status: "fleet_error",
      failure_label: "startup_posture",
      failure_detail: "fleet has no instructions configured",
    });
    // The live merge must name the failing check without a reload.
    expect(out[0]?.status).toBe("fleet_error");
    expect(out[0]?.outcome).toBe(
      "Failed a startup safety check — fleet has no instructions configured",
    );
  });

  it("EVENT_COMPLETE with empty failure fields keeps the plain status floor", () => {
    const seed = [evt({ id: "e9" })];
    const out = applyLiveFrame(seed, {
      kind: FRAME_KIND.EVENT_COMPLETE,
      event_id: "e9",
      status: "fleet_error",
      failure_label: "",
      failure_detail: "",
    });
    expect(out[0]?.outcome).toBe("The run failed.");
  });

  it("EVENT_COMPLETE falls back to processed when the wire omits status", () => {
    const seed = [evt({ id: "e9" })];
    // The backend can send a status-less completion frame; the timeline
    // must still mark the turn done rather than leave it 'received'.
    const frame = { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "e9" } as unknown as LiveFrame;
    expect(applyLiveFrame(seed, frame)[0]?.status).toBe("processed");
  });

  it("EVENT_COMPLETE for an unknown id, too thin to open a row, is a no-op", () => {
    const seed: FleetEvent[] = [];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "ghost", status: "processed" });
    expect(out).toBe(seed);
  });

  it("CHUNK with two events: only the matching event is updated; the other is returned unchanged", () => {
    // Two-element array exercises the `: e` (non-matching) arm of the map call.
    const bystander = evt({ id: "bystander", reply: "untouched" });
    const target = evt({ id: "target", reply: "start" });
    const seed = [bystander, target];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.CHUNK, event_id: "target", text: " more" });
    // The target event must have its reply extended.
    expect(out.find((e) => e.id === "target")?.reply).toBe("start more");
    // The bystander element must be the exact same object reference — not a copy.
    expect(out.find((e) => e.id === "bystander")).toBe(bystander);
  });

  it("EVENT_COMPLETE with two events: only the matching event's status changes; the other is unchanged", () => {
    // Two-element array exercises the `: e` (non-matching) arm of the map call.
    const bystander = evt({ id: "bystander", status: "received" });
    const target = evt({ id: "target", status: "received" });
    const seed = [bystander, target];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "target", status: "processed" });
    expect(out.find((e) => e.id === "target")?.status).toBe("processed");
    // The bystander must be the exact same object reference and retain its status.
    const bystanderOut = out.find((e) => e.id === "bystander");
    expect(bystanderOut).toBe(bystander);
    expect(bystanderOut?.status).toBe("received");
  });

  // ── Tool calls ──────────────────────────────────────────────────────────
  //
  // The backend has always published tool_call_started / _progress / _completed.
  // applyLiveFrame dropped all three through a `default: return prev`, while the
  // thread's own empty state promised "Tool calls, chunks, and completions appear
  // here as the fleet runs." The frames arrived; nothing kept them.

  // The merges locate their event once and copy the array once, the
  // shape `applyToolCall` already used. Reference identity is the observable
  // proof: a second pass would rebuild every element, not just the target.
  it("a chunk rebuilds only its own event and leaves every sibling reference intact", () => {
    const seed = [evt({ id: "a" }), evt({ id: "b", reply: "hi" }), evt({ id: "c" })];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.CHUNK, event_id: "b", text: " there" });

    expect(out).not.toBe(seed);
    expect(out[1]?.reply).toBe("hi there");
    expect(out[0]).toBe(seed[0]);
    expect(out[2]).toBe(seed[2]);
    expect(out[1]).not.toBe(seed[1]);
  });

  it("a completion rebuilds only its own event and carries the failure class", () => {
    const seed = [evt({ id: "a" }), evt({ id: "b" }), evt({ id: "c" })];
    const out = applyLiveFrame(seed, {
      kind: FRAME_KIND.EVENT_COMPLETE,
      event_id: "b",
      status: "fleet_error",
      failure_label: "startup_posture",
      failure_detail: "no instructions configured",
    });

    expect(out[0]).toBe(seed[0]);
    expect(out[2]).toBe(seed[2]);
    // The class rides alongside the sentence so guidance can render live.
    expect(out[1]?.failureLabel).toBe("startup_posture");
    expect(out[1]?.outcome).toContain("no instructions configured");
  });

  it("a clean completion carries no failure class", () => {
    const out = applyLiveFrame([evt({ id: "e1", failureLabel: "startup_posture" })], {
      kind: FRAME_KIND.EVENT_COMPLETE,
      event_id: "e1",
      status: "processed",
      failure_label: "",
      failure_detail: "",
    });
    expect(out[0]?.failureLabel).toBeNull();
  });

  it("a completion carrying no timing does not erase the elapsed a progress frame reported", () => {
    let out = applyLiveFrame([evt({ id: "e1" })], {
      kind: FRAME_KIND.TOOL_CALL_PROGRESS,
      event_id: "e1",
      name: "slow",
      elapsed_ms: 5_000,
    });
    out = applyLiveFrame(out, {
      kind: FRAME_KIND.TOOL_CALL_COMPLETED,
      event_id: "e1",
      name: "slow",
      ms: null as unknown as number,
    });
    expect(out[0]?.tools).toEqual([{ name: "slow", ms: 5_000, done: true }]);
  });
});
