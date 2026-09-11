import { describe, expect, it } from "vitest";
import { type EventRow, type LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { HEADLINE, OUTCOME } from "@/lib/events/event-summary";
import { applyLiveFrame } from "./fleet-stream-frames";
import { rowToEvent, type FleetEvent } from "./fleet-stream-row";

// The daemon's bracket frames carry more than a status: the opening bracket
// names the event's type and its own instant, and the completion carries the
// whole row. These are the reducers' contract with that shape — split from
// the frame suite by concern (figures and instants), not by size.

const OPENED_AT = Date.UTC(2026, 4, 15, 9, 0, 0);

function row(over: Partial<EventRow> = {}): EventRow {
  return {
    event_id: "e1",
    fleet_id: "z1",
    workspace_id: "ws1",
    actor: "cron:*",
    event_type: "cron",
    status: "processed",
    tokens: 1200,
    wall_ms: 12_000,
    failure_label: null,
    failure_detail: null,
    checkpoint_id: null,
    resumes_event_id: null,
    cost_nanos: 40_000_000,
    created_at: OPENED_AT,
    updated_at: OPENED_AT,
    ...over,
  };
}

function opened(over: Partial<Extract<LiveFrame, { kind: "event_received" }>> = {}): LiveFrame {
  return { kind: FRAME_KIND.EVENT_RECEIVED, event_id: "e1", actor: "cron:*", ...over };
}

function received(): FleetEvent[] {
  return applyLiveFrame([], opened({ event_type: "cron", created_at: OPENED_AT }));
}

describe("event_received — the opening bracket", () => {
  it("stamps the row with the daemon's instant, never the client clock", () => {
    const [event] = received();
    expect(event?.createdAt.getTime()).toBe(OPENED_AT);
  });

  it("captions a webhook or cron trigger by its type rather than a chat caption", () => {
    const [event] = applyLiveFrame([], opened({ actor: "webhook:github", event_type: "webhook" }));
    expect(event?.text).toBe(`webhook ${HEADLINE.RECEIVED_SUFFIX}`);
  });

  it("a frame with no type or instant still opens a row, on the neutral floor and the current clock", () => {
    const before = Date.now();
    const [event] = applyLiveFrame([], opened({ actor: "webhook:github" }));
    expect(event?.text).toBe(HEADLINE.EVENT_FALLBACK);
    expect(event?.createdAt.getTime()).toBeGreaterThanOrEqual(before);
    // No figures until the run reports.
    expect(event?.tokens).toBeUndefined();
  });
});

describe("event_received — a row the browser already holds", () => {
  it("adopts the daemon's instant on the operator's own steer, and keeps everything else", () => {
    const guessed = OPENED_AT - 90_000;
    const held: FleetEvent = {
      ...rowToEvent(row({ actor: "steer:kishore", status: "received", created_at: guessed })),
      text: "deploy it",
    };
    const [event] = applyLiveFrame([held], opened({ actor: "steer:kishore", created_at: OPENED_AT }));
    expect(event?.createdAt.getTime()).toBe(OPENED_AT);
    expect(event?.text).toBe("deploy it");
  });

  it("keeps the row's identity when the frame carries no instant or the same one", () => {
    const held = rowToEvent(row({ status: "received" }));
    expect(applyLiveFrame([held], opened())[0]).toBe(held);
    expect(applyLiveFrame([held], opened({ created_at: OPENED_AT }))[0]).toBe(held);
  });
});

describe("event_complete — the closing bracket", () => {
  it("folds the row's figures onto the event so the strip moves without a read", () => {
    const [event] = applyLiveFrame(received(), {
      kind: FRAME_KIND.EVENT_COMPLETE,
      ...row(),
      fleet_status: "active",
      pending_approvals: 0,
    });
    expect(event).toMatchObject({
      status: "processed",
      outcome: OUTCOME.COMPLETED,
      tokens: 1200,
      wallMs: 12_000,
      costNanos: 40_000_000,
    });
  });

  it("a run the daemon reported no telemetry for reads as unknown, never zero", () => {
    const [event] = applyLiveFrame(received(), {
      kind: FRAME_KIND.EVENT_COMPLETE,
      ...row({ tokens: null, wall_ms: null, cost_nanos: null }),
      fleet_status: "active",
      pending_approvals: 0,
    });
    expect(event).toMatchObject({ tokens: null, wallMs: null, costNanos: null });
  });

  it("a refusal closes the row with its label and no figures", () => {
    const [event] = applyLiveFrame(received(), {
      kind: FRAME_KIND.EVENT_COMPLETE,
      ...row({ status: "gate_blocked", failure_label: "balance_exhausted", tokens: null, wall_ms: null, cost_nanos: null }),
      fleet_status: "active",
      pending_approvals: 0,
    });
    expect(event?.status).toBe("gate_blocked");
    expect(event?.failureLabel).toBe("balance_exhausted");
    expect(event?.tokens).toBeNull();
  });
});

describe("event_complete — a row the timeline never opened", () => {
  it("opens the row from the frame, figures and all, so a late subscriber's strip still moves", () => {
    const events = applyLiveFrame([], {
      kind: FRAME_KIND.EVENT_COMPLETE,
      ...row({ actor: "continuation:e0", event_type: "continuation" }),
      fleet_status: "active",
      pending_approvals: 0,
    });
    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({
      id: "e1",
      status: "processed",
      tokens: 1200,
      wallMs: 12_000,
      costNanos: 40_000_000,
    });
    expect(events[0]?.createdAt.getTime()).toBe(OPENED_AT);
  });

  it("carries a continuation's checkpoint and resumed event, and dates an unstamped update by its creation", () => {
    const stamped = { ...row({ checkpoint_id: "ck_1", resumes_event_id: "e0" }) } as Partial<EventRow>;
    delete stamped.updated_at;
    const [event] = applyLiveFrame([], {
      kind: FRAME_KIND.EVENT_COMPLETE,
      ...stamped,
      event_id: "e1",
      fleet_status: "active",
      pending_approvals: 0,
    } as LiveFrame);
    expect(event?.createdAt.getTime()).toBe(OPENED_AT);
    expect(event?.id).toBe("e1");
  });

  it("drops a frame too thin to be a row rather than rendering a blank turn", () => {
    const thin = { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "e9", status: "processed" } as LiveFrame;
    expect(applyLiveFrame([], thin)).toEqual([]);
  });

  it("reads a malformed cause as absent instead of losing the frame", () => {
    const events = applyLiveFrame(received(), {
      kind: FRAME_KIND.EVENT_COMPLETE,
      ...row({ status: "fleet_error" }),
      failure_label: 12,
      failure_detail: { nested: true },
      fleet_status: "active",
      pending_approvals: 0,
    } as unknown as LiveFrame);
    expect(events[0]).toMatchObject({ status: "fleet_error", failureLabel: null, failureDetail: null });
  });

  it("a status the server never writes marks the turn done", () => {
    const events = applyLiveFrame(received(), {
      kind: FRAME_KIND.EVENT_COMPLETE,
      ...row({ status: "exploded" as EventRow["status"] }),
      fleet_status: "active",
      pending_approvals: 0,
    });
    expect(events[0]?.status).toBe("processed");
  });
});

describe("rowToEvent", () => {
  it("carries a durable row's figures so a backfilled completion moves the strip too", () => {
    expect(rowToEvent(row())).toMatchObject({ tokens: 1200, wallMs: 12_000, costNanos: 40_000_000 });
    expect(rowToEvent(row({ cost_nanos: null }))).toMatchObject({ costNanos: null });
  });
});
