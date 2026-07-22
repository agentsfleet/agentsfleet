import { describe, expect, it } from "vitest";

import { groupThreadEvents, groupTimeRange, MIN_GROUP_RUN } from "@/lib/events/event-grouping";
import { OUTCOME } from "@/lib/events/event-summary";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";

const WEBHOOK = "webhook:github";
const HEADLINE = "edited · agentsfleet/agentsfleet#541";
const FAILED_OUTCOME = "Failed a startup safety check — no instructions configured";

let seq = 0;

function evt(over: Partial<FleetEvent> = {}): FleetEvent {
  seq += 1;
  return {
    id: over.id ?? `e${seq}`,
    role: over.role ?? "system",
    actor: over.actor ?? WEBHOOK,
    text: over.text ?? HEADLINE,
    reply: over.reply ?? "",
    outcome: over.outcome ?? FAILED_OUTCOME,
    failureLabel: over.failureLabel ?? "startup_posture",
    failureDetail: over.failureDetail ?? null,
    createdAt: over.createdAt ?? new Date(Date.UTC(2026, 6, 22, 11, 38, 0)),
    status: over.status ?? "fleet_error",
    ...(over.custom ? { custom: over.custom } : {}),
  };
}

function run(count: number, over: Partial<FleetEvent> = {}): FleetEvent[] {
  return Array.from({ length: count }, () => evt(over));
}

describe("groupThreadEvents", () => {
  it("collapses a run of identical deliveries into one group holding every member", () => {
    const entries = groupThreadEvents(run(15));
    expect(entries).toHaveLength(1);
    const [group] = entries;
    expect(group?.kind).toBe("group");
    // Every swallowed event is still reachable — a group is a view, not a
    // count, so expanding it can hand back exactly what it hid.
    if (group?.kind === "group") expect(group.events).toHaveLength(15);
  });

  it("leaves a lone delivery as its own row", () => {
    const entries = groupThreadEvents(run(1));
    expect(entries).toHaveLength(1);
    expect(entries[0]?.kind).toBe("single");
  });

  it("needs a real repeat before it collapses anything", () => {
    // One is an event; the threshold is where "again" starts.
    expect(groupThreadEvents(run(MIN_GROUP_RUN - 1))[0]?.kind).toBe("single");
    expect(groupThreadEvents(run(MIN_GROUP_RUN))[0]?.kind).toBe("group");
  });

  it("never groups the operator's own messages or the fleet's replies", () => {
    const events = [
      evt({ role: "user", actor: "steer:user_abc", text: "why is this failing?", outcome: OUTCOME.NO_REPLY }),
      evt({ role: "user", actor: "steer:user_abc", text: "why is this failing?", outcome: OUTCOME.NO_REPLY }),
      evt({ role: "assistant", actor: "fleet", text: "", reply: "Because instructions are missing." }),
      evt({ role: "assistant", actor: "fleet", text: "", reply: "Because instructions are missing." }),
    ];
    const entries = groupThreadEvents(events);
    // Four rows in, four rows out: a person's words never become a count.
    expect(entries).toHaveLength(4);
    expect(entries.every((entry) => entry.kind === "single")).toBe(true);
  });

  it("breaks a run when the operator speaks in the middle of a burst", () => {
    const entries = groupThreadEvents([
      ...run(3),
      evt({ role: "user", actor: "steer:user_abc", text: "what is going on?" }),
      ...run(4),
    ]);
    expect(entries.map((entry) => entry.kind)).toEqual(["group", "single", "group"]);
  });

  it("breaks a run on a success between failures, and never reorders", () => {
    const events = [
      ...run(3),
      evt({ status: "processed", outcome: OUTCOME.NO_REPLY, failureLabel: null }),
      ...run(2),
    ];
    const entries = groupThreadEvents(events);
    expect(entries.map((entry) => entry.kind)).toEqual(["group", "single", "group"]);
    // Flattening the view must reproduce the input exactly — grouping is a
    // presentation of the array the stream owns, never a rewrite of it.
    const flattened = entries.flatMap((entry) =>
      entry.kind === "group" ? entry.events : [entry.event],
    );
    expect(flattened.map((event) => event.id)).toEqual(events.map((event) => event.id));
  });

  it("keeps two failures of the same class but different cause apart", () => {
    // The key carries the outcome sentence, which carries the cause. Merging
    // these would report one count for two genuinely different problems.
    const entries = groupThreadEvents([
      ...run(2, { outcome: "Failed a startup safety check — no instructions configured" }),
      ...run(2, { outcome: "Failed a startup safety check — no model configured" }),
    ]);
    expect(entries).toHaveLength(2);
    expect(entries.every((entry) => entry.kind === "group")).toBe(true);
  });

  it("keeps different actors apart even when they say the same thing", () => {
    const entries = groupThreadEvents([
      ...run(2, { actor: "webhook:github" }),
      ...run(2, { actor: "webhook:slack" }),
    ]);
    expect(entries).toHaveLength(2);
  });

  it("grows a group when another matching delivery lands, with no special case", () => {
    // A live frame needs no handling of its own: the function is pure over
    // the array, so the next render simply sees a longer run.
    const before = run(3);
    const grouped = groupThreadEvents(before);
    const after = groupThreadEvents([...before, evt()]);
    const first = grouped[0];
    const second = after[0];
    if (first?.kind !== "group" || second?.kind !== "group") throw new Error("expected groups");
    expect(first.events).toHaveLength(3);
    expect(second.events).toHaveLength(4);
    // Its identity is stable across the growth, so the row does not remount.
    expect(second.key).toBe(first.key);
  });

  it("survives an empty thread", () => {
    expect(groupThreadEvents([])).toEqual([]);
  });
});

describe("groupTimeRange", () => {
  it("reports the span a group covers, earliest first", () => {
    const early = new Date(Date.UTC(2026, 6, 22, 11, 38, 0));
    const late = new Date(Date.UTC(2026, 6, 22, 12, 3, 0));
    const range = groupTimeRange([evt({ createdAt: early }), evt({ createdAt: late })]);
    expect(range?.first).toEqual(early);
    expect(range?.last).toEqual(late);
  });

  it("orders the span even when the array runs newest-first", () => {
    const early = new Date(Date.UTC(2026, 6, 22, 11, 38, 0));
    const late = new Date(Date.UTC(2026, 6, 22, 12, 3, 0));
    const range = groupTimeRange([evt({ createdAt: late }), evt({ createdAt: early })]);
    expect(range?.first).toEqual(early);
    expect(range?.last).toEqual(late);
  });

  it("has no range to report for an empty group", () => {
    expect(groupTimeRange([])).toBeNull();
  });
});
