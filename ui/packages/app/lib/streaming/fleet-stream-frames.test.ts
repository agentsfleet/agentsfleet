import { describe, expect, it } from "vitest";
import { FRAME_KIND, type EventRow, type LiveFrame } from "@/lib/api/events";
import { HEADLINE, OUTCOME } from "@/lib/events/event-summary";
import {
  applyLiveFrame,
  maxServerCreatedAt,
  mergeBackfill,
  rfc3339Seconds,
  type FleetEvent,
} from "./fleet-stream-frames";

const MS_PER_SECOND = 1000 as const;

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
    reply: "",
    outcome: OUTCOME.WORKING,
    createdAt: new Date(2000),
    status: "received",
    ...over,
  };
}

describe("mergeBackfill", () => {
  it("dedupes by id and sorts the union oldest-first", () => {
    const prev = [evt({ id: "e2", createdAt: new Date(2000) })];
    const merged = mergeBackfill(prev, [
      row({ event_id: "e1", created_at: MS_PER_SECOND, response_text: "a" }),
      row({ event_id: "e2", created_at: 2000 }),
    ]);
    expect(merged.map((e) => e.id)).toEqual(["e1", "e2"]);
  });

  it("maps a null response_text with request context and a readable failure outcome", () => {
    const [first] = mergeBackfill([], [row({
      response_text: null,
      request_json: "{\"a\":1}",
      failure_label: "startup_posture",
    })]);
    expect(first?.text).toBe("");
    expect(first?.custom?.requestJson).toBe("{\"a\":1}");
    expect(first?.outcome).toBe("Failed a startup safety check");
  });

  it("replaces a partial live row with a terminal backfill row of the same id", () => {
    // An event that straddled an outage: live chunks accumulated a partial
    // text, the durable row carries the full final text + terminal status.
    const prev = [evt({ id: "e1", reply: "partial chu", status: "received" })];
    const merged = mergeBackfill(prev, [
      row({ event_id: "e1", status: "processed", response_text: "the full final text" }),
    ]);
    expect(merged).toHaveLength(1);
    expect(merged[0]?.reply).toBe("the full final text");
    expect(merged[0]?.status).toBe("processed");
  });

  it("retains live tool evidence when a terminal row is reconciled", () => {
    const tools = [{ name: "inspect", ms: 12, done: true }];
    const prev = [evt({ id: "e1", status: "received", tools })];
    const merged = mergeBackfill(prev, [
      row({ event_id: "e1", status: "processed", response_text: "done" }),
    ]);
    expect(merged[0]?.tools).toEqual(tools);
  });

  it("recovers the operator's own submitted text from the durable row", () => {
    // The reply field belongs to the fleet. An operator's message lives in the
    // stored request payload, so reading the reply field renders their own
    // message blank the moment the page reloads.
    const [first] = mergeBackfill(
      [],
      [
        row({
          actor: "steer:user_3gkbgxjnujsxbdxttcwcslpc87k",
          event_type: "chat",
          request_json: '{"message":"are you alive"}',
          response_text: null,
        }),
      ],
    );
    expect(first?.role).toBe("user");
    expect(first?.text).toBe("are you alive");
  });

  it("gives an integration event a headline instead of an empty body", () => {
    const [first] = mergeBackfill(
      [],
      [
        row({
          actor: "github-app",
          event_type: "webhook",
          request_json: JSON.stringify({ repo: "owner/repo", number: 12, action: "opened" }),
          response_text: null,
        }),
      ],
    );
    expect(first?.role).toBe("system");
    expect(first?.text).toBe("opened · owner/repo#12");
  });

  it("carries a non-empty outcome for a row with no body at all", () => {
    const [first] = mergeBackfill(
      [],
      [row({ response_text: null, status: "fleet_error", failure_label: "startup_posture" })],
    );
    expect(first?.text).toBe("");
    expect(first?.outcome).toBe("Failed a startup safety check");
  });

  it("keeps the live accumulation when the backfill row is still in progress", () => {
    // The live chunk stream is newer than the list snapshot for a running
    // event — a "received" backfill row must not clobber it.
    const prev = [evt({ id: "e1", reply: "live chunks so far", status: "received" })];
    const merged = mergeBackfill(prev, [
      row({ event_id: "e1", status: "received", response_text: "stale snapshot" }),
    ]);
    expect(merged).toHaveLength(1);
    expect(merged[0]?.reply).toBe("live chunks so far");
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
