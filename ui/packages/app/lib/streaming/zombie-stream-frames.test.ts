import { describe, expect, it } from "vitest";
import { FRAME_KIND, type EventRow, type LiveFrame } from "@/lib/api/events";
import {
  actorToRole,
  applyLiveFrame,
  mergeBackfill,
  type ZombieEvent,
} from "./zombie-stream-frames";

function row(over: Partial<EventRow> = {}): EventRow {
  return {
    event_id: "e1",
    zombie_id: "z1",
    workspace_id: "ws1",
    actor: "agent",
    event_type: "agent_run",
    status: "processed",
    request_json: "{}",
    response_text: "hello",
    tokens: null,
    wall_ms: null,
    failure_label: null,
    checkpoint_id: null,
    resumes_event_id: null,
    created_at: MS_PER_SECOND,
    updated_at: MS_PER_SECOND,
    ...over,
  } as EventRow;
}

function evt(over: Partial<ZombieEvent> = {}): ZombieEvent {
  return {
    id: "e0",
    role: "assistant",
    actor: "agent",
    text: "x",
    createdAt: new Date(2000),
    status: "received",
    ...over,
  };
}

describe("actorToRole", () => {
  it("maps steer:* to user, agent to assistant, everything else to system", () => {
    expect(actorToRole("steer:alice")).toBe("user");
    expect(actorToRole("agent")).toBe("assistant");
    expect(actorToRole("system")).toBe("system");
    expect(actorToRole("webhook")).toBe("system");
  });
});

describe("mergeBackfill", () => {
  it("drops rows already present and sorts the union oldest-first", () => {
    const prev = [evt({ id: "e2", createdAt: new Date(2000) })];
    const merged = mergeBackfill(prev, [
      row({ event_id: "e1", created_at: MS_PER_SECOND, response_text: "a" }),
      row({ event_id: "e2", created_at: 2000 }), // already in prev → skipped
    ]);
    expect(merged.map((e) => e.id)).toEqual(["e1", "e2"]);
  });

  it("maps a null response_text to an empty string and carries request_json", () => {
    const [first] = mergeBackfill([], [row({ response_text: null, request_json: "{\"a\":1}" })]);
    expect(first?.text).toBe("");
    expect(first?.custom?.requestJson).toBe("{\"a\":1}");
  });
});

describe("applyLiveFrame", () => {
  it("EVENT_RECEIVED appends a new event then dedupes a repeat by id", () => {
    const frame: LiveFrame = { kind: FRAME_KIND.EVENT_RECEIVED, event_id: "e1", actor: "steer:bob" };
    const once = applyLiveFrame([], frame);
    expect(once).toHaveLength(1);
    expect(once[0]?.role).toBe("user");
    const twice = applyLiveFrame(once, frame);
    expect(twice).toBe(once); // unchanged reference — no duplicate row
  });

  it("CHUNK creates an assistant event when none exists, then concatenates text", () => {
    const created = applyLiveFrame([], { kind: FRAME_KIND.CHUNK, event_id: "e9", text: "Hel" });
    expect(created[0]).toMatchObject({ role: "assistant", actor: "agent", text: "Hel" });
    const appended = applyLiveFrame(created, { kind: FRAME_KIND.CHUNK, event_id: "e9", text: "lo" });
    expect(appended[0]?.text).toBe("Hello");
  });

  it("CHUNK keeps a user-role event as user while concatenating", () => {
    const seed = [evt({ id: "e9", role: "user", actor: "steer:x", text: "Hi " })];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.CHUNK, event_id: "e9", text: "there" });
    expect(out[0]).toMatchObject({ role: "user", text: "Hi there" });
  });

  it("EVENT_COMPLETE sets the reported status", () => {
    const seed = [evt({ id: "e9" })];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "e9", status: "gate_blocked" });
    expect(out[0]?.status).toBe("gate_blocked");
  });

  it("EVENT_COMPLETE falls back to processed when the wire omits status", () => {
    const seed = [evt({ id: "e9" })];
    // The backend can send a status-less completion frame; the timeline
    // must still mark the turn done rather than leave it 'received'.
    const frame = { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "e9" } as unknown as LiveFrame;
    expect(applyLiveFrame(seed, frame)[0]?.status).toBe("processed");
  });

  it("EVENT_COMPLETE for an unknown id is a no-op", () => {
    const seed: ZombieEvent[] = [];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "ghost", status: "processed" });
    expect(out).toBe(seed);
  });

  it("ignores frame kinds the timeline does not render", () => {
    const seed: ZombieEvent[] = [];
    const frame: LiveFrame = {
      kind: FRAME_KIND.TOOL_CALL_STARTED,
      event_id: "e1",
      name: "shell",
      args_redacted: {},
    };
    expect(applyLiveFrame(seed, frame)).toBe(seed);
  });

  it("CHUNK with two events: only the matching event is updated; the other is returned unchanged", () => {
    // Two-element array exercises the `: e` (non-matching) arm of the map call.
    const bystander = evt({ id: "bystander", text: "untouched" });
    const target = evt({ id: "target", text: "start" });
    const seed = [bystander, target];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.CHUNK, event_id: "target", text: " more" });
    // The target event must have its text extended.
    expect(out.find((e) => e.id === "target")?.text).toBe("start more");
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
});
const MS_PER_SECOND = 1000 as const;
