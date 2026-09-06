import { describe, expect, it } from "vitest";
import { type LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { NO_FACTS } from "@/lib/events/run-summary";
import { factsOf, mergeFacts } from "./fleet-stream-facts";
import { figure, text } from "./fleet-stream-row";

describe("figure", () => {
  it("keeps a finite number and reads anything else as unknown", () => {
    expect(figure(0)).toBe(0);
    expect(figure(1200)).toBe(1200);
    for (const notAFigure of [null, undefined, "1200", Number.NaN, Number.POSITIVE_INFINITY, {}]) {
      expect(figure(notAFigure)).toBeNull();
    }
  });
});

describe("text", () => {
  it("keeps a trimmed string and reads anything else as empty", () => {
    expect(text("  balance_exhausted ")).toBe("balance_exhausted");
    for (const notText of [null, undefined, 12, {}, ["x"]]) {
      expect(text(notText)).toBe("");
    }
  });
});

describe("factsOf", () => {
  it("a completion carries both fleet facts", () => {
    const frame: LiveFrame = {
      kind: FRAME_KIND.EVENT_COMPLETE,
      event_id: "e1",
      fleet_status: "paused",
      pending_approvals: 2,
    };
    expect(factsOf(frame)).toEqual({ status: "paused", pendingApprovals: 2 });
  });

  it("a gate frame carries the count alone — a gate changes nothing about the lifecycle", () => {
    expect(
      factsOf({ kind: FRAME_KIND.GATE_OPENED, gate_id: "g", event_id: "e", pending_approvals: 1 }),
    ).toEqual({ pendingApprovals: 1 });
    expect(
      factsOf({
        kind: FRAME_KIND.GATE_RESOLVED,
        gate_id: "g",
        event_id: "e",
        status: "denied",
        resolved_by: "human:x",
        pending_approvals: 0,
      }),
    ).toEqual({ pendingApprovals: 0 });
  });

  it("a completion with unreadable facts says nothing rather than something false", () => {
    const frame = { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "e1" } as LiveFrame;
    expect(factsOf(frame)).toEqual({ status: null, pendingApprovals: null });
  });

  it("every other frame carries no fleet fact", () => {
    expect(factsOf({ kind: FRAME_KIND.CHUNK, event_id: "e", text: "hi" })).toEqual({});
    expect(factsOf({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "e", actor: "fleet" })).toEqual({});
    expect(factsOf({ kind: FRAME_KIND.HELLO, fleet_ids: [] })).toEqual({});
  });
});

describe("factsOf — the status is a fleet status or nothing", () => {
  it("an empty or unknown spelling reads as unknown, never as a status the strip would show", () => {
    for (const notAStatus of ["", "ACTIVE", "exploded", 7, null]) {
      const frame = {
        kind: FRAME_KIND.EVENT_COMPLETE,
        event_id: "e1",
        fleet_status: notAStatus,
        pending_approvals: 0,
      } as unknown as LiveFrame;
      expect(factsOf(frame)).toEqual({ status: null, pendingApprovals: 0 });
    }
    // And an unknown never erases the status already held.
    const held = { status: "active", pendingApprovals: 1 };
    expect(mergeFacts(held, { status: null, pendingApprovals: 0 })).toEqual({
      status: "active",
      pendingApprovals: 0,
    });
  });
});

describe("mergeFacts", () => {
  it("keeps identity when the patch restates what is held", () => {
    const held = { status: "active", pendingApprovals: 1 };
    expect(mergeFacts(held, { pendingApprovals: 1 })).toBe(held);
    expect(mergeFacts(held, {})).toBe(held);
  });

  it("a null in the patch never erases a fact already held", () => {
    const held = { status: "active", pendingApprovals: 1 };
    expect(mergeFacts(held, { status: null, pendingApprovals: null })).toBe(held);
  });

  it("folds a new fact in and leaves the other as it was", () => {
    expect(mergeFacts(NO_FACTS, { status: "paused" })).toEqual({ status: "paused", pendingApprovals: null });
    expect(mergeFacts({ status: "paused", pendingApprovals: 3 }, { pendingApprovals: 2 })).toEqual({
      status: "paused",
      pendingApprovals: 2,
    });
  });
});
