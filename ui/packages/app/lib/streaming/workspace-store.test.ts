import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { WorkspaceControlFrame, WorkspaceLiveFrame } from "@/lib/api/events";
import type { BackfillOutcome, WorkspaceBackfillRequest } from "@/lib/streaming/fleet-stream-backfill";

import { FRAME_KIND } from "@/lib/api/events-types";
import { MAX_LIVE_EVENTS } from "@/lib/streaming/fleet-stream-cap";

// The store is proven against captured subscriptions rather than a fake
// EventSource: the claims here are about what the store does with a frame it
// was handed, and the transport that hands it over has its own suite.
type FrameListener = (frame: WorkspaceLiveFrame) => void;
type ControlListener = (frame: WorkspaceControlFrame) => void;
type BackfillFn = (workspaceId: string, anchorMs: number | null) => void;

const fleetListeners = new Map<string, FrameListener>();
let controlListener: ControlListener | null = null;
let backfill: BackfillFn | null = null;

vi.mock("@/lib/streaming/workspace-stream", () => ({
  WORKSPACE_CONNECTION_STATUS: { CONNECTING: "connecting", LIVE: "live", RECONNECTING: "reconnecting" },
  noteServerFrameTime: vi.fn(),
  subscribeStatus: (_workspaceId: string, _listener: unknown, onReconnect: BackfillFn) => {
    backfill = onReconnect;
    return () => {};
  },
  subscribeWorkspaceFrames: (_workspaceId: string, listener: ControlListener) => {
    controlListener = listener;
    return () => {};
  },
  subscribeFleet: (_workspaceId: string, fleetId: string, listener: FrameListener) => {
    fleetListeners.set(fleetId, listener);
    return () => {};
  },
}));

const runWorkspaceBackfill = vi.fn<(req: WorkspaceBackfillRequest) => Promise<BackfillOutcome>>();
vi.mock("@/lib/streaming/fleet-stream-backfill", () => ({
  runWorkspaceBackfill: (req: WorkspaceBackfillRequest) => runWorkspaceBackfill(req),
  warnBackfillFailure: vi.fn(),
}));

import { WorkspaceStore } from "./workspace-store";

const WORKSPACE_ID = "ws_store";
const FLEET_A = "fleet_a";
const FLEET_B = "fleet_b";
const SEVEN_EVENTS = 7;
const SPENT_NANOS = 1_500_000_000;
const STANDING = { events_processed: 3, budget_used_nanos: 900_000_000 };
const OVERFLOW = MAX_LIVE_EVENTS + 50;
const CREATED_AT_MS = 1_700_000_000_000;

function received(fleetId: string, eventId: string, counters: object = {}): WorkspaceLiveFrame {
  return {
    kind: FRAME_KIND.EVENT_RECEIVED,
    fleet_id: fleetId,
    event_id: eventId,
    actor: "fleet",
    created_at: CREATED_AT_MS,
    ...counters,
  };
}

function completed(fleetId: string, eventId: string, counters: object = {}): WorkspaceLiveFrame {
  return {
    kind: FRAME_KIND.EVENT_COMPLETE,
    fleet_id: fleetId,
    event_id: eventId,
    status: "processed",
    created_at: CREATED_AT_MS,
    updated_at: CREATED_AT_MS,
    ...counters,
  };
}

function push(fleetId: string, frame: WorkspaceLiveFrame) {
  const listener = fleetListeners.get(fleetId);
  if (!listener) throw new Error(`no subscription for ${fleetId}`);
  listener(frame);
}

function greet(frame: WorkspaceControlFrame) {
  if (!controlListener) throw new Error("no workspace subscription");
  controlListener(frame);
}

let store: WorkspaceStore;
let disconnect: () => void;
let queuedFrame: FrameRequestCallback | null = null;

function flushFrame() {
  const callback = queuedFrame;
  queuedFrame = null;
  callback?.(0);
}

beforeEach(() => {
  fleetListeners.clear();
  controlListener = null;
  backfill = null;
  queuedFrame = null;
  vi.stubGlobal(
    "requestAnimationFrame",
    vi.fn((callback: FrameRequestCallback) => {
      queuedFrame = callback;
      return 1;
    }),
  );
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  store = new WorkspaceStore(WORKSPACE_ID);
  disconnect = store.connect([FLEET_A, FLEET_B]);
});

afterEach(() => {
  disconnect();
  vi.unstubAllGlobals();
  runWorkspaceBackfill.mockReset();
});

describe("the tile counters are a snapshot the store assigns", () => {
  it("a_repeated_frame_leaves_the_tile_unchanged", () => {
    // Dimension 1.1. The same frame twice carries the same truth twice; a
    // delta would read 14, a snapshot reads 7.
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

  it("the hello assigns each announced fleet, and leaves an unannounced one standing", () => {
    // Dimension 1.2, the client half: the figures a late subscriber missed
    // arrive on the greeting, before any event frame. A fleet the map omits
    // keeps what it had — here nothing, so the server render still stands.
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

describe("the event map stays bounded", () => {
  it("the_store_still_bounds_its_event_map_without_absorb", () => {
    // Dimension 3.1. `absorb` was the only thing that ever emptied a fleet's
    // rows; with it gone the cap has to hold on the write itself.
    for (let index = 0; index < OVERFLOW; index += 1) {
      push(FLEET_A, received(FLEET_A, `e${index}`));
      push(FLEET_A, completed(FLEET_A, `e${index}`));
    }
    expect(store.snapshot(FLEET_A).events.length).toBeLessThanOrEqual(MAX_LIVE_EVENTS);
    // And a key only for a fleet the wall subscribed: the map cannot grow
    // with the workspace, only with the tiles on screen.
    expect(store.snapshot(FLEET_B).events).toHaveLength(0);
  });

  it("a reconnect backfill is capped on the same write", async () => {
    runWorkspaceBackfill.mockImplementation(async (req) => {
      req.onPage(
        Array.from({ length: OVERFLOW }, (_, index) => ({
          event_id: `b${index}`,
          fleet_id: FLEET_A,
          workspace_id: WORKSPACE_ID,
          actor: "fleet",
          event_type: "chat",
          status: "processed",
          tokens: null,
          wall_ms: null,
          failure_label: null,
          failure_detail: null,
          checkpoint_id: null,
          resumes_event_id: null,
          cost_nanos: null,
          created_at: CREATED_AT_MS + index,
          updated_at: CREATED_AT_MS + index,
        })),
      );
      return { ok: true, watermark: null };
    });
    if (!backfill) throw new Error("no status subscription");
    backfill(WORKSPACE_ID, null);
    await vi.waitFor(() => expect(runWorkspaceBackfill).toHaveBeenCalledTimes(1));
    expect(store.snapshot(FLEET_A).events.length).toBeLessThanOrEqual(MAX_LIVE_EVENTS);
  });
});
