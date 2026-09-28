import { describe, expect, it } from "vitest";
import { renderHook } from "@testing-library/react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";
import { useFleetThreadEntries, type FleetThreadEntry } from "./useFleetThreadEntries";

// What a streaming frame costs the settled history above it. assistant-ui
// caches each converted message by the object it was handed (core
// `thread-message-converter.ts`), so a settled turn is converted again only
// if its entry object changed. The mirror below keys the same way.

const SETTLED_TURNS = 100;
// An operator turn splits into its trigger and its `:reply` message.
const ENTRIES_PER_TURN = 2;

function operatorTurn(index: number): FleetEvent {
  return evt({ id: `turn-${index}`, role: "user", actor: "steer:user_1", text: `ask ${index}`, reply: `answer ${index}`, status: "processed" });
}

function convertEvent(event: FleetEvent): ThreadMessageLike {
  return { role: event.role, id: event.id, content: [{ type: "text", text: event.text }] };
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
  it("test_settled_entries_keep_identity_across_frames", () => {
    const settled = Array.from({ length: SETTLED_TURNS }, (_, index) => operatorTurn(index));
    const streaming = evt({ id: "live", role: "user", actor: "steer:user_1", text: "go", reply: "wor", status: "received" });
    const hook = renderHook(({ events }) => useFleetThreadEntries(events, convertEvent), {
      initialProps: { events: [...settled, streaming] },
    });
    const before = hook.result.current.entries;
    expect(before).toHaveLength((SETTLED_TURNS + 1) * ENTRIES_PER_TURN);
    const mirror = converterMirror(hook.result.current.convertEntry);
    mirror.pass(before);
    const first = mirror.conversions;

    // One chunk lands on the streaming reply: the stream replaces that event
    // and hands back every other one by reference.
    hook.rerender({ events: [...settled, { ...streaming, reply: "world" }] });
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
    const streaming = evt({ id: "live", role: "user", actor: "steer:user_1", text: "go", reply: "wor", status: "received" });
    const hook = renderHook(({ events }) => useFleetThreadEntries(events, convertEvent), {
      initialProps: { events: [streaming] },
    });
    const [trigger, reply] = hook.result.current.entries;

    hook.rerender({ events: [{ ...streaming, reply: "world", reasoning: "thinking", thinking: true }] });
    const [sameTrigger, nextReply] = hook.result.current.entries;
    expect(sameTrigger).toBe(trigger);
    expect(nextReply).not.toBe(reply);

    // A change the trigger renders gives it a new object.
    hook.rerender({ events: [{ ...streaming, reply: "world", status: "processed" }] });
    expect(hook.result.current.entries[0]).not.toBe(trigger);
  });
});
