import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { FleetCountersSnapshot } from "@/lib/api/events";

import { FRAME_KIND } from "@/lib/api/events-types";
import {
  completed,
  flushFrame,
  greet,
  push,
  lastGreeting,
  received,
  setupWorkspaceWire,
  WORKSPACE_ID,
} from "@/tests/helpers/workspace-store-harness";
import { WorkspaceStore } from "./workspace-store";

const FLEET_A = "fleet_a";
const FLEET_B = "fleet_b";
const SEVEN_EVENTS = 7;
const SPENT_NANOS = 1_500_000_000;
const STANDING = { events_processed: 3, budget_used_nanos: 900_000_000 };

let store: WorkspaceStore;
let disconnect: () => void;

setupWorkspaceWire();

beforeEach(() => {
  store = new WorkspaceStore(WORKSPACE_ID);
  disconnect = store.connect([FLEET_A, FLEET_B]);
});

afterEach(() => disconnect());

describe("the tile counters are a snapshot the store assigns", () => {
  it("a_repeated_frame_leaves_the_tile_unchanged", () => {
    // The same frame twice carries the same truth twice; a delta would read
    // 14, a snapshot reads 7.
    const frame = completed(FLEET_A, "e1", {
      events_processed: SEVEN_EVENTS,
      budget_used_nanos: SPENT_NANOS,
    });
    push(FLEET_A, frame);
    push(FLEET_A, frame);
    expect(store.snapshot(FLEET_A).counters).toEqual({
      eventsProcessed: SEVEN_EVENTS,
      spentNanos: SPENT_NANOS,
    });
  });

  it("a frame that crossed the greeting in flight cannot walk the tile backwards", () => {
    // The hello read the counters after a frame was already queued behind
    // the subscription; the frame lands second carrying the OLDER figures.
    // Both counters only grow on the server, so the greater one stands.
    greet({
      kind: FRAME_KIND.HELLO,
      fleet_ids: [FLEET_A],
      counters: { [FLEET_A]: { events_processed: SEVEN_EVENTS, budget_used_nanos: SPENT_NANOS } },
    });
    push(FLEET_A, completed(FLEET_A, "e6", STANDING));
    expect(store.snapshot(FLEET_A).counters).toEqual({
      eventsProcessed: SEVEN_EVENTS,
      spentNanos: SPENT_NANOS,
    });
    // And a frame that is genuinely newer still moves it forward.
    push(FLEET_A, completed(FLEET_A, "e8", {
      events_processed: SEVEN_EVENTS + 1,
      budget_used_nanos: SPENT_NANOS,
    }));
    expect(store.snapshot(FLEET_A).counters?.eventsProcessed).toBe(SEVEN_EVENTS + 1);
  });

  it("the hello assigns each announced fleet, and leaves an unannounced one standing", () => {
    // The figures a late subscriber missed arrive on the greeting, before any
    // event frame. A fleet the map omits keeps what it had — here nothing, so
    // the server render still stands.
    push(FLEET_B, received(FLEET_B, "e0", STANDING));
    greet({
      kind: FRAME_KIND.HELLO,
      fleet_ids: [FLEET_A, FLEET_B],
      counters: { [FLEET_A]: { events_processed: SEVEN_EVENTS, budget_used_nanos: SPENT_NANOS } },
    });
    expect(store.snapshot(FLEET_A).counters).toEqual({
      eventsProcessed: SEVEN_EVENTS,
      spentNanos: SPENT_NANOS,
    });
    expect(store.snapshot(FLEET_B).counters).toEqual({
      eventsProcessed: STANDING.events_processed,
      spentNanos: STANDING.budget_used_nanos,
    });
  });

  it("a malformed hello map neither throws nor assigns, and a stray key is dropped", () => {
    // The wire is untrusted. A null entry, a primitive, a negative or
    // fractional figure, and a fleet the wall never subscribed all fall away
    // without breaking the greeting for the fleets that are well formed.
    greet({
      kind: FRAME_KIND.HELLO,
      fleet_ids: [FLEET_A, FLEET_B],
      counters: {
        [FLEET_A]: null,
        [FLEET_B]: { events_processed: SEVEN_EVENTS, budget_used_nanos: SPENT_NANOS },
        fleet_stranger: { events_processed: 1, budget_used_nanos: 1 },
      } as unknown as Record<string, FleetCountersSnapshot>,
    });
    expect(store.snapshot(FLEET_A).helloReceived).toBe(true);
    expect(store.snapshot(FLEET_A).counters).toBeUndefined();
    expect(store.snapshot(FLEET_B).counters).toEqual({
      eventsProcessed: SEVEN_EVENTS,
      spentNanos: SPENT_NANOS,
    });
    expect(store.snapshot("fleet_stranger").counters).toBeUndefined();

    greet({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A], counters: "nonsense" as never });
    push(FLEET_A, completed(FLEET_A, "e1", { events_processed: -1, budget_used_nanos: 1 }));
    push(FLEET_A, completed(FLEET_A, "e1", { events_processed: 1.5, budget_used_nanos: 1 }));
    expect(store.snapshot(FLEET_A).counters).toBeUndefined();
  });

  it("a hello without a counters map announces the set and assigns nothing", () => {
    greet({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    expect(store.snapshot(FLEET_A).helloReceived).toBe(true);
    expect(store.snapshot(FLEET_A).counters).toBeUndefined();
  });

  it("a frame the daemon could not fill leaves the standing figures, never zeros", () => {
    push(FLEET_A, received(FLEET_A, "e1", STANDING));
    // The read did not answer: the pair is absent, not zero.
    push(FLEET_A, completed(FLEET_A, "e1"));
    expect(store.snapshot(FLEET_A).counters).toEqual({
      eventsProcessed: STANDING.events_processed,
      spentNanos: STANDING.budget_used_nanos,
    });
  });

  it("half a pair is malformed and is refused whole", () => {
    push(FLEET_A, received(FLEET_A, "e1", STANDING));
    push(FLEET_A, completed(FLEET_A, "e1", { events_processed: SEVEN_EVENTS }));
    push(FLEET_A, completed(FLEET_A, "e1", { budget_used_nanos: SPENT_NANOS }));
    expect(store.snapshot(FLEET_A).counters).toEqual({
      eventsProcessed: STANDING.events_processed,
      spentNanos: STANDING.budget_used_nanos,
    });
  });

  it("a runner's mid-run frame says nothing about where the fleet stands", () => {
    push(FLEET_A, received(FLEET_A, "e1", STANDING));
    push(FLEET_A, { kind: FRAME_KIND.CHUNK, fleet_id: FLEET_A, event_id: "e1", text: "…" });
    expect(store.snapshot(FLEET_A).counters).toEqual({
      eventsProcessed: STANDING.events_processed,
      spentNanos: STANDING.budget_used_nanos,
    });
  });

  it("the gate frames carry the snapshot too", () => {
    push(FLEET_A, {
      kind: FRAME_KIND.GATE_OPENED,
      fleet_id: FLEET_A,
      gate_id: "g1",
      event_id: "e1",
      pending_approvals: 1,
      events_processed: SEVEN_EVENTS,
      budget_used_nanos: SPENT_NANOS,
    });
    expect(store.snapshot(FLEET_A).counters?.eventsProcessed).toBe(SEVEN_EVENTS);
    push(FLEET_A, {
      kind: FRAME_KIND.GATE_RESOLVED,
      fleet_id: FLEET_A,
      gate_id: "g1",
      event_id: "e1",
      status: "approved",
      resolved_by: "someone",
      pending_approvals: 0,
      events_processed: SEVEN_EVENTS + 1,
      budget_used_nanos: SPENT_NANOS,
    });
    expect(store.snapshot(FLEET_A).counters?.eventsProcessed).toBe(SEVEN_EVENTS + 1);
  });
});

describe("the wall's own subscription", () => {
  it("hears the greeting once, and nothing after it unsubscribes", () => {
    const listener = vi.fn();
    const unsubscribe = store.subscribeWorkspace(listener);
    greet({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    flushFrame();
    expect(listener).toHaveBeenCalledTimes(1);
    expect(store.workspaceSnapshot().helloReceived).toBe(true);
    // Cached by identity between changes: `useSyncExternalStore` re-renders
    // on a fresh object, so two reads with nothing in between are one object.
    expect(store.workspaceSnapshot()).toBe(store.workspaceSnapshot());

    unsubscribe();
    greet({ kind: FRAME_KIND.CATCHING_UP, dropped: 2 });
    flushFrame();
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

describe("joining after the greeting", () => {
  it("an ungreeted store takes the set the connection last announced", () => {
    lastGreeting.mockReturnValue({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    const late = new WorkspaceStore(WORKSPACE_ID);
    const leave = late.connect([FLEET_A, FLEET_B]);

    expect(late.workspaceSnapshot().helloReceived).toBe(true);
    expect(late.snapshot(FLEET_A).isLive).toBe(true);
    expect(late.snapshot(FLEET_B).isLive).toBe(false);
    leave();
  });

  it("a greeted store re-subscribing for a changed set keeps catching up", () => {
    greet({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    greet({ kind: FRAME_KIND.CATCHING_UP, dropped: 2 });
    lastGreeting.mockReturnValue({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    disconnect();
    disconnect = store.connect([FLEET_A, FLEET_B, "fleet_c"]);

    expect(store.workspaceSnapshot().catchingUp).toBe(true);
  });
});
