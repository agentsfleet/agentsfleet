import { isDeepStrictEqual } from "node:util";
import { describe, expect, it } from "vitest";
import { renderHook } from "@testing-library/react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import { AGENTSFLEET_TURN_STATUS, type FleetTurn } from "@/lib/streaming/fleet-stream-row";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";
import { convertTurn as renderedTrigger } from "../stream/useFleetTurnStream";
import { sameValues, useFleetThreadEntries, type FleetThreadEntry } from "./useFleetThreadEntries";

// What a streaming frame costs the settled history above it. assistant-ui
// caches each converted message by the object it was handed (core
// `thread-message-converter.ts`), so a settled turn is converted again only
// if its entry object changed. The mirror below keys the same way.

const { PROCESSED, RECEIVED } = AGENTSFLEET_TURN_STATUS;
const OPERATOR = "steer:user_1";
const USER = "user" as const;
const LIVE = "live";
const ASK = "go";
const PARTIAL = "wor";
const ANSWER = "world";
const REASONING = "thinking";
const REQUEST_GO = JSON.stringify({ message: ASK });
const SETTLED_TURNS = 100;
// An operator turn splits into its trigger and its `:reply` message.
const ENTRIES_PER_TURN = 2;

function operatorTurn(index: number): FleetTurn {
  return evt({ id: `turn-${index}`, role: USER, actor: OPERATOR, text: `ask ${index}`, reply: `answer ${index}`, status: PROCESSED });
}

function convertTurn(turn: FleetTurn): ThreadMessageLike {
  return { role: turn.role, id: turn.id, content: [{ type: "text", text: turn.text }] };
}

// The library's cache, reduced to what matters here: a message converts once
// per entry object.
function converterMirror(convert: (entry: FleetThreadEntry) => ThreadMessageLike) {
  const cache = new WeakMap<FleetThreadEntry, ThreadMessageLike>();
  let conversions = 0;
  return {
    pass(entries: FleetThreadEntry[]) {
      for (const entry of entries) {
        if (cache.has(entry)) continue;
        conversions += 1;
        cache.set(entry, convert(entry));
      }
    },
    get conversions() {
      return conversions;
    },
  };
}

describe("useFleetThreadEntries", () => {
  it("keeps empty restarted integration replies visible and separate", () => {
    const turns = ["first", "second"].map((id) => evt({
      id, role: "system", actor: "webhook:github", text: "delivery", reply: "", status: RECEIVED,
    }));
    const hook = renderHook(({ rows }) => useFleetThreadEntries(rows, renderedTrigger), {
      initialProps: { rows: turns },
    });
    expect(hook.result.current.entries.map((entry) => entry.kind)).toEqual(["group"]);
    hook.rerender({ rows: turns.map((turn) => ({ ...turn, runRestarted: true })) });
    expect(hook.result.current.entries.map((entry) => entry.kind)).toEqual(["trigger", "reply", "trigger", "reply"]);
    for (const entry of hook.result.current.entries.filter((row) => row.kind === "reply")) {
      const message = hook.result.current.convertEntry(entry);
      expect(message.metadata?.custom).toEqual(expect.objectContaining({ runRestarted: true }));
    }
  });

  it("test_settled_entries_keep_identity_across_frames", () => {
    const settled = Array.from({ length: SETTLED_TURNS }, (_, index) => operatorTurn(index));
    const streaming = evt({ id: LIVE, role: USER, actor: OPERATOR, text: ASK, reply: PARTIAL, status: RECEIVED });
    const hook = renderHook(({ turns }) => useFleetThreadEntries(turns, convertTurn), {
      initialProps: { turns: [...settled, streaming] },
    });
    const before = hook.result.current.entries;
    expect(before).toHaveLength((SETTLED_TURNS + 1) * ENTRIES_PER_TURN);
    const mirror = converterMirror(hook.result.current.convertEntry);
    mirror.pass(before);
    const first = mirror.conversions;

    // One chunk lands on the streaming reply: the stream replaces that turn
    // and hands back every other one by reference.
    hook.rerender({ turns: [...settled, { ...streaming, reply: ANSWER }] });
    const after = hook.result.current.entries;
    const settledCount = SETTLED_TURNS * ENTRIES_PER_TURN;
    for (let index = 0; index < settledCount; index += 1) {
      expect(after[index]).toBe(before[index]);
    }

    mirror.pass(after);
    // Only the streaming turn's reply converts again.
    expect(mirror.conversions - first).toBe(1);
  });

  it("test_reply_delta_keeps_trigger_identity", () => {
    const streaming = evt({ id: LIVE, role: USER, actor: OPERATOR, text: ASK, reply: PARTIAL, status: RECEIVED });
    const hook = renderHook(({ turns }) => useFleetThreadEntries(turns, renderedTrigger), {
      initialProps: { turns: [streaming] },
    });
    const [trigger, reply] = hook.result.current.entries;

    hook.rerender({ turns: [{ ...streaming, reply: ANSWER, reasoning: REASONING, thinking: true }] });
    const [sameTrigger, nextReply] = hook.result.current.entries;
    expect(sameTrigger).toBe(trigger);
    expect(nextReply).not.toBe(reply);

    // A change the trigger renders gives it a new object.
    hook.rerender({ turns: [{ ...streaming, reply: ANSWER, status: PROCESSED }] });
    expect(hook.result.current.entries[0]).not.toBe(trigger);
  });

  it("test_trigger_identity_follows_every_rendered_field", () => {
    // Each change touches one field of a streaming operator turn. The trigger
    // keeps its object exactly when the chat's own converter renders the turn
    // the same. `id` and `role` are left out: they change the entry's key and
    // split, not only its trigger.
    const base = evt({
      id: "turn", role: USER, actor: OPERATOR, text: ASK, reply: PARTIAL, status: RECEIVED,
      custom: { requestJson: REQUEST_GO },
    });
    const rendered: Partial<FleetTurn>[] = [
      { createdAt: new Date(3_000) }, { text: "go again" }, { actor: "steer:user_2" },
      { custom: { requestJson: JSON.stringify({ message: "stop" }) } }, { status: PROCESSED }, { clientTimestamp: true },
      { submittedAtMs: 12 }, { replyRecovering: true }, { runRestarted: true }, { restartAttempt: "42" }, { outcome: "Stopped." },
      { failureLabel: "startup_posture" }, { failureDetail: "posture check failed" },
    ];
    const unrendered: Partial<FleetTurn>[] = [
      { reply: ANSWER }, { reasoning: REASONING }, { thinking: true }, { reasoningStartedAtMs: 5 },
      { reasoningEndedAtMs: 6 }, { tools: [] }, { tokens: 3 }, { wallMs: 4 }, { costNanos: "5" },
      // Equal values in fresh objects render the same.
      { createdAt: new Date(base.createdAt.getTime()) }, { custom: { requestJson: REQUEST_GO } },
    ];
    for (const [changes, kept] of [[rendered, false], [unrendered, true]] as const) {
      for (const change of changes) {
        const next = { ...base, ...change };
        const hook = renderHook(({ turns }) => useFleetThreadEntries(turns, renderedTrigger), {
          initialProps: { turns: [base] },
        });
        const [trigger] = hook.result.current.entries;
        hook.rerender({ turns: [next] });
        const reused = hook.result.current.entries[0] === trigger;
        expect(reused, JSON.stringify(change)).toBe(kept);
        expect(reused, JSON.stringify(change)).toBe(isDeepStrictEqual(renderedTrigger(base), renderedTrigger(next)));
        hook.unmount();
      }
    }
  });
});

describe("sameValues", () => {
  const DAY_ONE = new Date(1);
  it.each<[string, unknown, unknown, boolean]>([
    ["one value", 1, 1, true],
    ["one instant in two dates", DAY_ONE, new Date(1), true],
    ["two instants", DAY_ONE, new Date(2), false],
    ["a date against a number", DAY_ONE, 1, false],
    ["a number against a date", 1, DAY_ONE, false],
    ["two strings", "a", "b", false],
    ["an object against a string", {}, "a", false],
    ["null against an object", null, {}, false],
    ["an object against null", {}, null, false],
    ["nested equal arrays", [1, [2]], [1, [2]], true],
    ["arrays of two lengths", [1], [1, 2], false],
    ["objects with different keys", { a: undefined }, { b: 1 }, false],
    ["objects with different values", { a: 1 }, { a: 2 }, false],
    ["nested equal objects", { a: { b: DAY_ONE } }, { a: { b: new Date(1) } }, true],
  ])("compares %s", (_name, a, b, same) => {
    expect(sameValues(a, b)).toBe(same);
  });
});
