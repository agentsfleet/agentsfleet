import { describe, expect, it } from "vitest";
import { type LiveFrame } from "@/lib/api/turns";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { HEADLINE, OUTCOME } from "@/lib/turns/turn-summary";
import { applyLiveFrame } from "./fleet-stream-frames";
import type { FleetTurn } from "./fleet-stream-row";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";

// The opening, the chunks and the closing: how the bracket and reply frames
// fold into the timeline, and that each rebuilds only its own row. Tool
// frames are `fleet-stream-frames.tools.test.ts`; the merge is the base suite.

describe("applyLiveFrame", () => {
  it("TURN_RECEIVED appends a new turn then dedupes a repeat by id", () => {
    const frame: LiveFrame = { kind: FRAME_KIND.TURN_RECEIVED, turn_id: "e1", actor: "steer:bob" };
    const once = applyLiveFrame([], frame);
    expect(once).toHaveLength(1);
    expect(once[0]?.role).toBe("user");
    const twice = applyLiveFrame(once, frame);
    expect(twice).toBe(once); // unchanged reference — no duplicate row
  });

  it("TURN_RECEIVED for a webhook actor renders the neutral floor, not a chat caption", () => {
    // The frame carries no payload and no turn kind — fabricating "chat"
    // captioned webhook/cron turns as "chat received" until reload.
    const frame: LiveFrame = { kind: FRAME_KIND.TURN_RECEIVED, turn_id: "e2", actor: "webhook:github" };
    const out = applyLiveFrame([], frame);
    expect(out[0]).toMatchObject({ role: "system", text: HEADLINE.TURN_FALLBACK });
  });

  it("CHUNK creates an assistant turn when none exists, accumulating into the reply", () => {
    const created = applyLiveFrame([], { text_kind: "answer", kind: FRAME_KIND.CHUNK, turn_id: "e9", text: "Hel" });
    expect(created[0]).toMatchObject({ role: "assistant", actor: "fleet", text: "", reply: "Hel" });
    const appended = applyLiveFrame(created, { text_kind: "answer", kind: FRAME_KIND.CHUNK, turn_id: "e9", text: "lo" });
    // The chunk stream is the fleet's reply — it never becomes the trigger text.
    expect(appended[0]?.reply).toBe("Hello");
    expect(appended[0]?.text).toBe("");
  });

  it("CHUNK on an operator turn appends to the reply, never the operator's own text", () => {
    const seed = [evt({ id: "e9", role: "user", actor: "steer:x", text: "Hi ", reply: "" })];
    const out = applyLiveFrame(seed, { text_kind: "answer", kind: FRAME_KIND.CHUNK, turn_id: "e9", text: "there" });
    expect(out[0]).toMatchObject({ role: "user", text: "Hi ", reply: "there" });
  });

  it("TURN_COMPLETE sets the reported status", () => {
    const seed = [evt({ id: "e9" })];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.TURN_COMPLETE, turn_id: "e9", status: "gate_blocked" });
    expect(out[0]?.status).toBe("gate_blocked");
  });

  it("TURN_COMPLETE carries the failure cause into the outcome live", () => {
    const seed = [evt({ id: "e9" })];
    const out = applyLiveFrame(seed, {
      kind: FRAME_KIND.TURN_COMPLETE,
      turn_id: "e9",
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

  it("TURN_COMPLETE with empty failure fields keeps the plain status floor", () => {
    const seed = [evt({ id: "e9" })];
    const out = applyLiveFrame(seed, {
      kind: FRAME_KIND.TURN_COMPLETE,
      turn_id: "e9",
      status: "fleet_error",
      failure_label: "",
      failure_detail: "",
    });
    expect(out[0]?.outcome).toBe("The run failed.");
  });

  it("TURN_COMPLETE falls back to processed when the wire omits status", () => {
    const seed = [evt({ id: "e9" })];
    // The backend can send a status-less completion frame; the timeline
    // must still mark the turn done rather than leave it 'received'.
    const frame = { kind: FRAME_KIND.TURN_COMPLETE, turn_id: "e9" } as unknown as LiveFrame;
    expect(applyLiveFrame(seed, frame)[0]?.status).toBe("processed");
  });

  it("TURN_COMPLETE for an unknown id, too thin to open a row, is a no-op", () => {
    const seed: FleetTurn[] = [];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.TURN_COMPLETE, turn_id: "ghost", status: "processed" });
    expect(out).toBe(seed);
  });

  it("CHUNK with two turns: only the matching turn is updated; the other is returned unchanged", () => {
    // Two-element array exercises the `: e` (non-matching) arm of the map call.
    const bystander = evt({ id: "bystander", reply: "untouched" });
    const target = evt({ id: "target", reply: "start" });
    const seed = [bystander, target];
    const out = applyLiveFrame(seed, { text_kind: "answer", kind: FRAME_KIND.CHUNK, turn_id: "target", text: " more" });
    // The target turn must have its reply extended.
    expect(out.find((e) => e.id === "target")?.reply).toBe("start more");
    // The bystander element must be the exact same object reference — not a copy.
    expect(out.find((e) => e.id === "bystander")).toBe(bystander);
  });

  it("TURN_COMPLETE with two turns: only the matching turn's status changes; the other is unchanged", () => {
    // Two-element array exercises the `: e` (non-matching) arm of the map call.
    const bystander = evt({ id: "bystander", status: "received" });
    const target = evt({ id: "target", status: "received" });
    const seed = [bystander, target];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.TURN_COMPLETE, turn_id: "target", status: "processed" });
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

  // The merges locate their turn once and copy the array once, the
  // shape `applyToolStep` already used. Reference identity is the observable
  // proof: a second pass would rebuild every element, not just the target.
  it("a chunk rebuilds only its own turn and leaves every sibling reference intact", () => {
    const seed = [evt({ id: "a" }), evt({ id: "b", reply: "hi" }), evt({ id: "c" })];
    const out = applyLiveFrame(seed, { text_kind: "answer", kind: FRAME_KIND.CHUNK, turn_id: "b", text: " there" });

    expect(out).not.toBe(seed);
    expect(out[1]?.reply).toBe("hi there");
    expect(out[0]).toBe(seed[0]);
    expect(out[2]).toBe(seed[2]);
    expect(out[1]).not.toBe(seed[1]);
  });

  it("a completion rebuilds only its own turn and carries the failure class", () => {
    const seed = [evt({ id: "a" }), evt({ id: "b" }), evt({ id: "c" })];
    const out = applyLiveFrame(seed, {
      kind: FRAME_KIND.TURN_COMPLETE,
      turn_id: "b",
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
      kind: FRAME_KIND.TURN_COMPLETE,
      turn_id: "e1",
      status: "processed",
      failure_label: "",
      failure_detail: "",
    });
    expect(out[0]?.failureLabel).toBeNull();
  });

  it("a malformed completion without timing leaves the pending call unchanged", () => {
    const progressAt = 1_000;
    let out = applyLiveFrame([evt({ id: "e1" })], { call_id: "slow",
      kind: FRAME_KIND.TOOL_CALL_PROGRESS,
      turn_id: "e1",
      name: "slow",
      elapsed_ms: 5_000,
    }, progressAt);
    out = applyLiveFrame(out, { call_id: "slow", status: "succeeded",
      kind: FRAME_KIND.TOOL_CALL_COMPLETED,
      turn_id: "e1",
      name: "slow",
      ms: null as unknown as number,
    }, progressAt + 5_000);
    expect(out[0]?.tools).toEqual([{ name: "slow", startedAtMs: progressAt, callId: "slow", ms: 5_000, done: false }]);
  });
});
