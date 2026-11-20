import { describe, expect, it } from "vitest";

import { groupThreadTurns, MIN_GROUP_RUN } from "@/lib/turns/turn-grouping";
import { OUTCOME } from "@/lib/turns/turn-summary";
import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";

const WEBHOOK = "webhook:github";
const HEADLINE = "edited · agentsfleet/agentsfleet#541";
const FAILED_OUTCOME = "Failed a startup safety check — no instructions configured";

let seq = 0;

function evt(over: Partial<FleetTurn> = {}): FleetTurn {
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
    ...(over.tools ? { tools: over.tools } : {}),
    ...(over.reasoning ? { reasoning: over.reasoning } : {}),
  };
}

function run(count: number, over: Partial<FleetTurn> = {}): FleetTurn[] {
  return Array.from({ length: count }, () => evt(over));
}

describe("groupThreadTurns", () => {
  it("collapses a run of identical deliveries into one group holding every member", () => {
    const entries = groupThreadTurns(run(15));
    expect(entries).toHaveLength(1);
    const [group] = entries;
    expect(group?.kind).toBe("group");
    // Every swallowed turn is still reachable — a group is a view, not a
    // count, so expanding it can hand back exactly what it hid.
    if (group?.kind === "group") expect(group.turns).toHaveLength(15);
  });

  it("leaves a lone delivery as its own row", () => {
    const entries = groupThreadTurns(run(1));
    expect(entries).toHaveLength(1);
    expect(entries[0]?.kind).toBe("single");
  });

  it("needs a real repeat before it collapses anything", () => {
    // One is a turn; the threshold is where "again" starts.
    expect(groupThreadTurns(run(MIN_GROUP_RUN - 1))[0]?.kind).toBe("single");
    expect(groupThreadTurns(run(MIN_GROUP_RUN))[0]?.kind).toBe("group");
  });

  it("never groups the operator's own messages or the fleet's replies", () => {
    const turns = [
      evt({ role: "user", actor: "steer:user_abc", text: "why is this failing?", outcome: OUTCOME.COMPLETED }),
      evt({ role: "user", actor: "steer:user_abc", text: "why is this failing?", outcome: OUTCOME.COMPLETED }),
      evt({ role: "assistant", actor: "fleet", text: "", reply: "Because instructions are missing." }),
      evt({ role: "assistant", actor: "fleet", text: "", reply: "Because instructions are missing." }),
    ];
    const entries = groupThreadTurns(turns);
    // Four rows in, four rows out: a person's words never become a count.
    expect(entries).toHaveLength(4);
    expect(entries.every((entry) => entry.kind === "single")).toBe(true);
  });

  it("test_turns_with_parts_never_group", () => {
    // Integration turns that reasoned or called a tool carry a reply row of
    // their own; a group shows members as ticks with no parts, so grouping
    // them would hide what the fleet did.
    const withTools = run(2, { tools: [{ name: "read_file", startedAtMs: 1, ms: 5, done: true }] });
    expect(groupThreadTurns(withTools).map((entry) => entry.kind)).toEqual(["single", "single"]);
    const withReasoning = run(2, { reasoning: "checked the signature" });
    expect(groupThreadTurns(withReasoning).map((entry) => entry.kind)).toEqual(["single", "single"]);
    // Bare activity still coalesces.
    expect(groupThreadTurns(run(2)).map((entry) => entry.kind)).toEqual(["group"]);
  });

  it("breaks a run when the operator speaks in the middle of a burst", () => {
    const entries = groupThreadTurns([
      ...run(3),
      evt({ role: "user", actor: "steer:user_abc", text: "what is going on?" }),
      ...run(4),
    ]);
    expect(entries.map((entry) => entry.kind)).toEqual(["group", "single", "group"]);
  });

  it("breaks a run on a success between failures, and never reorders", () => {
    const turns = [
      ...run(3),
      evt({ status: "processed", outcome: OUTCOME.COMPLETED, failureLabel: null }),
      ...run(2),
    ];
    const entries = groupThreadTurns(turns);
    expect(entries.map((entry) => entry.kind)).toEqual(["group", "single", "group"]);
    // Flattening the view must reproduce the input exactly — grouping is a
    // presentation of the array the stream owns, never a rewrite of it.
    const flattened = entries.flatMap((entry) =>
      entry.kind === "group" ? entry.turns : [entry.turn],
    );
    expect(flattened.map((turn) => turn.id)).toEqual(turns.map((turn) => turn.id));
  });

  it("keeps two failures of the same class but different cause apart", () => {
    // The key carries the outcome sentence, which carries the cause. Merging
    // these would report one count for two genuinely different problems.
    const entries = groupThreadTurns([
      ...run(2, { outcome: "Failed a startup safety check — no instructions configured" }),
      ...run(2, { outcome: "Failed a startup safety check — no model configured" }),
    ]);
    expect(entries).toHaveLength(2);
    expect(entries.every((entry) => entry.kind === "group")).toBe(true);
  });

  it("keeps different actors apart even when they say the same thing", () => {
    const entries = groupThreadTurns([
      ...run(2, { actor: "webhook:github" }),
      ...run(2, { actor: "webhook:slack" }),
    ]);
    expect(entries).toHaveLength(2);
  });

  it("grows a group when another matching delivery lands, with no special case", () => {
    // A live frame needs no handling of its own: the function is pure over
    // the array, so the next render simply sees a longer run.
    const before = run(3);
    const grouped = groupThreadTurns(before);
    const after = groupThreadTurns([...before, evt()]);
    const first = grouped[0];
    const second = after[0];
    if (first?.kind !== "group" || second?.kind !== "group") throw new Error("expected groups");
    expect(first.turns).toHaveLength(3);
    expect(second.turns).toHaveLength(4);
    // Its identity is stable across the growth, so the row does not remount.
    expect(second.key).toBe(first.key);
  });

  it("hands back unchanged entries by reference so untouched rows can bail out", () => {
    // A streaming reply regroups the whole thread on every frame. If every
    // entry came back new, React would re-render every row per chunk.
    const stable = run(3);
    const tail = evt({ actor: "webhook:slack" });
    const before = groupThreadTurns([...stable, tail]);
    const changedTail = { ...tail, reply: "partial…" };
    const after = groupThreadTurns([...stable, changedTail], before);

    expect(after[0]).toBe(before[0]);
    expect(after[1]).not.toBe(before[1]);
  });

  it("is unchanged in output whether or not a previous result is supplied", () => {
    const turns = [...run(3), evt({ actor: "webhook:slack" })];
    const fresh = groupThreadTurns(turns);
    const reused = groupThreadTurns(turns, groupThreadTurns(turns));
    expect(reused.map((e) => e.key)).toEqual(fresh.map((e) => e.key));
  });

  it("survives an empty thread", () => {
    expect(groupThreadTurns([])).toEqual([]);
  });
});
