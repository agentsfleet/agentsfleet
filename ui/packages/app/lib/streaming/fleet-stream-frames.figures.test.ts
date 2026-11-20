import { describe, expect, it } from "vitest";
import { type TurnRow, type LiveFrame } from "@/lib/api/turns";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { HEADLINE, OUTCOME } from "@/lib/turns/turn-summary";
import { ACTIVITY_KIND, FLEET_ACTIVITY } from "@/lib/turns/fleet-activity";
import { applyLiveFrame } from "./fleet-stream-frames";
import { rowToTurn, type FleetTurn } from "./fleet-stream-row";

// The daemon's bracket frames carry more than a status: the opening bracket
// names the turn's type and its own instant, and the completion carries the
// whole row. These are the reducers' contract with that shape — split from
// the frame suite by concern (figures and instants), not by size.

const OPENED_AT = Date.UTC(2026, 4, 15, 9, 0, 0);

function row(over: Partial<TurnRow> = {}): TurnRow {
  return {
    turn_id: "e1",
    fleet_id: "z1",
    workspace_id: "ws1",
    actor: "cron:*",
    event_type: "cron",
    status: "processed",
    tokens: 1200,
    wall_ms: 12_000,
    failure_label: null,
    failure_detail: null,
    resumes_turn_id: null,
    cost_nanos: "40000000",
    created_at: new Date(OPENED_AT).toISOString(),
    updated_at: new Date(OPENED_AT).toISOString(),
    ...over,
  };
}

function opened(over: Partial<Extract<LiveFrame, { kind: "turn_received" }>> = {}): LiveFrame {
  return { kind: FRAME_KIND.TURN_RECEIVED, turn_id: "e1", actor: "cron:*", ...over };
}

function received(): FleetTurn[] {
  return applyLiveFrame([], opened({ event_type: "cron", created_at: new Date(OPENED_AT).toISOString() }));
}

describe("turn_received — the opening bracket", () => {
  it("stamps the row with the daemon's instant, never the client clock", () => {
    const [turn] = received();
    expect(turn?.createdAt.getTime()).toBe(OPENED_AT);
  });

  it("captions a webhook or cron trigger by its type rather than a chat caption", () => {
    const [turn] = applyLiveFrame([], opened({ actor: "webhook:github", event_type: "webhook" }));
    expect(turn?.text).toBe(`webhook ${HEADLINE.RECEIVED_SUFFIX}`);
  });

  it("a frame with no type or instant still opens a row, on the neutral floor and the current clock", () => {
    const before = Date.now();
    const [turn] = applyLiveFrame([], opened({ actor: "webhook:github" }));
    expect(turn?.text).toBe(HEADLINE.TURN_FALLBACK);
    expect(turn?.createdAt.getTime()).toBeGreaterThanOrEqual(before);
    // No figures until the run reports.
    expect(turn?.tokens).toBeUndefined();
  });
});

describe("turn_received — a row the browser already holds", () => {
  it("adopts the daemon's instant on the operator's own steer, and keeps everything else", () => {
    const guessed = OPENED_AT - 90_000;
    const held: FleetTurn = {
      ...rowToTurn(row({ actor: "steer:kishore", status: "received", created_at: new Date(guessed).toISOString() })),
      text: "deploy it",
    };
    const [turn] = applyLiveFrame([held], opened({ actor: "steer:kishore", created_at: new Date(OPENED_AT).toISOString() }));
    expect(turn?.createdAt.getTime()).toBe(OPENED_AT);
    expect(turn?.text).toBe("deploy it");
  });

  it("keeps the row's identity when the frame carries no instant or the same one", () => {
    const held = rowToTurn(row({ status: "received" }));
    expect(applyLiveFrame([held], opened())[0]).toBe(held);
    expect(applyLiveFrame([held], opened({ created_at: new Date(OPENED_AT).toISOString() }))[0]).toBe(held);
  });
});

describe("turn_complete — the closing bracket", () => {
  it("folds the row's figures onto the turn so the strip moves without a read", () => {
    const [turn] = applyLiveFrame(received(), {
      kind: FRAME_KIND.TURN_COMPLETE,
      ...row(),
      fleet_status: "active",
      pending_approvals: 0,
    });
    expect(turn).toMatchObject({
      status: "processed",
      outcome: OUTCOME.COMPLETED,
      tokens: 1200,
      wallMs: 12_000,
      costNanos: "40000000",
    });
  });

  it("a run the daemon reported no telemetry for reads as unknown, never zero", () => {
    const [turn] = applyLiveFrame(received(), {
      kind: FRAME_KIND.TURN_COMPLETE,
      ...row({ tokens: null, wall_ms: null, cost_nanos: null }),
      fleet_status: "active",
      pending_approvals: 0,
    });
    expect(turn).toMatchObject({ tokens: null, wallMs: null, costNanos: null });
  });

  it("retains exact streamed costs above safe integers", () => {
    const [turn] = applyLiveFrame(received(), { kind: FRAME_KIND.TURN_COMPLETE,
      ...row({ cost_nanos: "9007199254740993" }), fleet_status: "active", pending_approvals: 0 });
    expect(turn?.costNanos).toBe("9007199254740993");
  });

  it("a numeric stream cost remains unknown instead of showing an imprecise charge", () => {
    const [turn] = applyLiveFrame(received(), { kind: FRAME_KIND.TURN_COMPLETE,
      ...row(), cost_nanos: 9007199254740992, fleet_status: "active", pending_approvals: 0 } as unknown as LiveFrame);
    expect(turn?.costNanos).toBeNull();
  });

  it("a refusal closes the row with its label and no figures", () => {
    const [turn] = applyLiveFrame(received(), {
      kind: FRAME_KIND.TURN_COMPLETE,
      ...row({ status: "gate_blocked", failure_label: "balance_exhausted", tokens: null, wall_ms: null, cost_nanos: null }),
      fleet_status: "active",
      pending_approvals: 0,
    });
    expect(turn?.status).toBe("gate_blocked");
    expect(turn?.failureLabel).toBe("balance_exhausted");
    expect(turn?.tokens).toBeNull();
  });
});

describe("turn_complete — a row the timeline never opened", () => {
  it("opens the row from the frame, figures and all, so a late subscriber's strip still moves", () => {
    const turns = applyLiveFrame([], {
      kind: FRAME_KIND.TURN_COMPLETE,
      ...row({ actor: "continuation:e0", event_type: "continuation" }),
      fleet_status: "active",
      pending_approvals: 0,
    });
    expect(turns).toHaveLength(1);
    expect(turns[0]).toMatchObject({
      id: "e1",
      status: "processed",
      tokens: 1200,
      wallMs: 12_000,
      costNanos: "40000000",
    });
    expect(turns[0]?.createdAt.getTime()).toBe(OPENED_AT);
  });

  it("carries a continuation's resumed turn, and dates an unstamped update by its creation", () => {
    const stamped = { ...row({ resumes_turn_id: "e0" }) } as Partial<TurnRow>;
    delete stamped.updated_at;
    const [turn] = applyLiveFrame([], {
      kind: FRAME_KIND.TURN_COMPLETE,
      ...stamped,
      turn_id: "e1",
      fleet_status: "active",
      pending_approvals: 0,
    } as LiveFrame);
    expect(turn?.createdAt.getTime()).toBe(OPENED_AT);
    expect(turn?.id).toBe("e1");
  });

  it("drops a frame too thin to be a row rather than rendering a blank turn", () => {
    const thin = { kind: FRAME_KIND.TURN_COMPLETE, turn_id: "e9", status: "processed" } as LiveFrame;
    expect(applyLiveFrame([], thin)).toEqual([]);
  });

  it("reads a malformed cause as absent instead of losing the frame", () => {
    const turns = applyLiveFrame(received(), {
      kind: FRAME_KIND.TURN_COMPLETE,
      ...row({ status: "fleet_error" }),
      failure_label: 12,
      failure_detail: { nested: true },
      fleet_status: "active",
      pending_approvals: 0,
    } as unknown as LiveFrame);
    expect(turns[0]).toMatchObject({ status: "fleet_error", failureLabel: null, failureDetail: null });
  });

  it("a status the server never writes marks the turn done", () => {
    const turns = applyLiveFrame(received(), {
      kind: FRAME_KIND.TURN_COMPLETE,
      ...row({ status: "exploded" as TurnRow["status"] }),
      fleet_status: "active",
      pending_approvals: 0,
    });
    expect(turns[0]?.status).toBe("processed");
  });
});

describe("rowToTurn", () => {
  // Value: protects=backfilled receipts stay system activity without a fabricated completed run;
  // fails_when=recorded status gets the ordinary completion floor; seam=none; no_claim=live transport.
  it("keeps a saved setup receipt separate from a completed fleet answer", () => {
    const receipt = {...row({actor: "config_reload", event_type: FLEET_ACTIVITY.type, status: FLEET_ACTIVITY.status,
      tokens: null, wall_ms: null, cost_nanos: null}),
      request_json: JSON.stringify({kind: ACTIVITY_KIND.setupSaved, revision: "3"})};
    expect(rowToTurn(receipt)).toMatchObject({role: "system", text: "Setup saved · v3", reply: "", outcome: "",
      status: FLEET_ACTIVITY.status, tokens: null, wallMs: null, costNanos: null});
    expect(rowToTurn(row()).outcome).toBe(OUTCOME.COMPLETED);
  });

  it("carries a durable row's figures so a backfilled completion moves the strip too", () => {
    expect(rowToTurn(row())).toMatchObject({ tokens: 1200, wallMs: 12_000, costNanos: "40000000" });
    expect(rowToTurn(row({ cost_nanos: null }))).toMatchObject({ costNanos: null });
  });
});
