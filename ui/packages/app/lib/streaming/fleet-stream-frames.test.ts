import { describe, expect, it } from "vitest";
import { FRAME_KIND, type EventRow, type LiveFrame } from "@/lib/api/events";
import {
  actorToRole,
  applyLiveFrame,
  maxServerCreatedAt,
  mergeBackfill,
  rfc3339Seconds,
  type FleetEvent,
} from "./fleet-stream-frames";

function row(over: Partial<EventRow> = {}): EventRow {
  return {
    event_id: "e1",
    fleet_id: "z1",
    workspace_id: "ws1",
    actor: "fleet",
    event_type: "fleet_run",
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

function evt(over: Partial<FleetEvent> = {}): FleetEvent {
  return {
    id: "e0",
    role: "assistant",
    actor: "fleet",
    text: "x",
    createdAt: new Date(2000),
    status: "received",
    ...over,
  };
}

describe("actorToRole", () => {
  it("maps steer:* to user, fleet to assistant, everything else to system", () => {
    expect(actorToRole("steer:alice")).toBe("user");
    expect(actorToRole("fleet")).toBe("assistant");
    expect(actorToRole("system")).toBe("system");
    expect(actorToRole("webhook")).toBe("system");
  });
});

describe("mergeBackfill", () => {
  it("dedupes by id and sorts the union oldest-first", () => {
    const prev = [evt({ id: "e2", createdAt: new Date(2000) })];
    const merged = mergeBackfill(prev, [
      row({ event_id: "e1", created_at: MS_PER_SECOND, response_text: "a" }),
      row({ event_id: "e2", created_at: 2000 }),
    ]);
    expect(merged.map((e) => e.id)).toEqual(["e1", "e2"]);
  });

  it("maps a null response_text to an empty string and carries request_json", () => {
    const [first] = mergeBackfill([], [row({ response_text: null, request_json: "{\"a\":1}" })]);
    expect(first?.text).toBe("");
    expect(first?.custom?.requestJson).toBe("{\"a\":1}");
  });

  it("replaces a partial live row with a terminal backfill row of the same id", () => {
    // An event that straddled an outage: live chunks accumulated a partial
    // text, the durable row carries the full final text + terminal status.
    const prev = [evt({ id: "e1", text: "partial chu", status: "received" })];
    const merged = mergeBackfill(prev, [
      row({ event_id: "e1", status: "processed", response_text: "the full final text" }),
    ]);
    expect(merged).toHaveLength(1);
    expect(merged[0]?.text).toBe("the full final text");
    expect(merged[0]?.status).toBe("processed");
  });

  it("keeps the live accumulation when the backfill row is still in progress", () => {
    // The live chunk stream is newer than the list snapshot for a running
    // event — a "received" backfill row must not clobber it.
    const prev = [evt({ id: "e1", text: "live chunks so far", status: "received" })];
    const merged = mergeBackfill(prev, [
      row({ event_id: "e1", status: "received", response_text: "stale snapshot" }),
    ]);
    expect(merged).toHaveLength(1);
    expect(merged[0]?.text).toBe("live chunks so far");
  });
});

describe("maxServerCreatedAt", () => {
  it("folds the newest created_at into the watermark and ignores non-numeric values", () => {
    expect(maxServerCreatedAt(null, [])).toBeNull();
    expect(
      maxServerCreatedAt(null, [row({ created_at: 5_000 }), row({ created_at: 3_000 })]),
    ).toBe(5_000);
    expect(maxServerCreatedAt(7_000, [row({ created_at: 5_000 })])).toBe(7_000);
    expect(
      maxServerCreatedAt(null, [row({ created_at: "bogus" as unknown as number })]),
    ).toBeNull();
  });
});

describe("rfc3339Seconds", () => {
  it("truncates to the 20-char second-granular shape and clamps negatives", () => {
    expect(rfc3339Seconds(Date.UTC(2026, 4, 15, 18, 29, 58, 789))).toBe("2026-05-15T18:29:58Z");
    expect(rfc3339Seconds(-5)).toBe("1970-01-01T00:00:00Z");
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
    expect(created[0]).toMatchObject({ role: "assistant", actor: "fleet", text: "Hel" });
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
    const seed: FleetEvent[] = [];
    const out = applyLiveFrame(seed, { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "ghost", status: "processed" });
    expect(out).toBe(seed);
  });

  it("ignores frame kinds the timeline does not render", () => {
    const seed: FleetEvent[] = [];
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
