import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  __resetRegistryForTests,
  CONNECTION_STATUS,
  appendOptimistic,
  getSnapshot,
  markOptimisticFailed,
  reconcileOptimistic,
  subscribe,
} from "./fleet-stream-registry";
import type { EventRow } from "@/lib/api/events";

// Mirrors the FakeEventSource pattern in tests/use-fleet-event-stream.test.ts.
// Centralizing was considered and rejected — the helper is small and the
// duplication keeps each test file freestanding.
class FakeEventSource {
  static instances: FakeEventSource[] = [];
  url: string;
  onopen: ((this: EventSource, ev: Event) => unknown) | null = null;
  onmessage: ((this: EventSource, ev: MessageEvent) => unknown) | null = null;
  onerror: ((this: EventSource, ev: Event) => unknown) | null = null;
  closed = false;
  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }
  close() {
    this.closed = true;
  }
}

function row(over: Partial<EventRow> = {}): EventRow {
  const now = Date.UTC(2026, 4, 15, 18, 30, 0);
  return {
    event_id: "evt_seed",
    fleet_id: "zomb_a",
    workspace_id: "ws_1",
    actor: "alice@example.com",
    event_type: "chat",
    status: "processed",
    request_json: "{}",
    response_text: "seed body",
    tokens: 1,
    wall_ms: 10,
    failure_label: null,
    checkpoint_id: null,
    resumes_event_id: null,
    created_at: now,
    updated_at: now,
    ...over,
  };
}

const WS = "ws_1";
const Z_A = "zomb_a";
const Z_B = "zomb_b";
const NO_SEED: EventRow[] = [];
const IDLE_RELEASE_MS = 30_000;

beforeEach(() => {
  vi.useFakeTimers();
  FakeEventSource.instances = [];
  (globalThis as unknown as { EventSource: unknown }).EventSource = FakeEventSource;
  __resetRegistryForTests();
});

afterEach(() => {
  __resetRegistryForTests();
  vi.useRealTimers();
  delete (globalThis as { EventSource?: unknown }).EventSource;
});

describe("fleet-stream-registry — subscribe lifecycle", () => {
  it("opens a single EventSource per fleetId regardless of subscriber count", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const b = subscribe(WS, Z_A, NO_SEED, () => {});
    expect(FakeEventSource.instances.length).toBe(1);
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
    a();
    b();
  });

  it("notifies every active listener when the snapshot changes", () => {
    const l1 = vi.fn();
    const l2 = vi.fn();
    const a = subscribe(WS, Z_A, NO_SEED, l1);
    const b = subscribe(WS, Z_A, NO_SEED, l2);
    const es = FakeEventSource.instances[0]!;
    es.onopen?.call(es as unknown as EventSource, {} as Event);
    expect(l1).toHaveBeenCalled();
    expect(l2).toHaveBeenCalled();
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
    a();
    b();
  });
});

describe("fleet-stream-registry — server-rendered seed", () => {
  it("seeds the event list from the initial rows and sorts ascending", () => {
    const t0 = Date.UTC(2026, 4, 15, 18, 0, 0);
    const t1 = Date.UTC(2026, 4, 15, 18, 30, 0);
    const a = subscribe(WS, Z_A, [
      row({ event_id: "evt_newer", created_at: t1 }),
      row({ event_id: "evt_older", created_at: t0 }),
    ], () => {});
    const snap = getSnapshot(Z_A);
    expect(snap.events.map((e) => e.id)).toEqual(["evt_older", "evt_newer"]);
    a();
  });

  it("seeds nothing (no client backfill GET) when initial is empty", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    expect(getSnapshot(Z_A).events).toEqual([]);
    // A single cookie-authed SSE connection opens; no bearer-authed fetch.
    expect(FakeEventSource.instances.length).toBe(1);
    a();
  });

  it("ignores the second subscriber's initial rows — the live entry is authoritative", () => {
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_first" })], () => {});
    const b = subscribe(WS, Z_A, [row({ event_id: "evt_second" })], () => {});
    const ids = getSnapshot(Z_A).events.map((e) => e.id);
    expect(ids).toEqual(["evt_first"]);
    a();
    b();
  });
});

describe("fleet-stream-registry — refcount + idle release", () => {
  it("keeps the EventSource alive when one of two subscribers detaches", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const b = subscribe(WS, Z_A, NO_SEED, () => {});
    a();
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
    b();
  });

  it("starts an idle timer (not an immediate close) when refcount hits zero", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    a();
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
    vi.advanceTimersByTime(IDLE_RELEASE_MS - 1);
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
  });

  it("tears the EventSource down once the idle window elapses with no resubscribe", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    a();
    vi.advanceTimersByTime(IDLE_RELEASE_MS + 1);
    expect(FakeEventSource.instances[0]!.closed).toBe(true);
  });

  it("survives a same-fleet revisit within the idle window — no new EventSource", () => {
    // Same-fleet /dashboard ↔ /fleets/[id] round-trip is the load-bearing
    // DX case: the EventSource must NOT reconnect when the user comes back
    // within IDLE_RELEASE_MS.
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    a();
    vi.advanceTimersByTime(IDLE_RELEASE_MS / 2);
    const b = subscribe(WS, Z_A, NO_SEED, () => {});
    expect(FakeEventSource.instances.length).toBe(1);
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
    vi.advanceTimersByTime(IDLE_RELEASE_MS * 2);
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
    b();
  });

  it("opens a fresh EventSource on cross-fleet subscription", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const b = subscribe(WS, Z_B, NO_SEED, () => {});
    expect(FakeEventSource.instances.length).toBe(2);
    expect(FakeEventSource.instances[0]!.url).toContain(Z_A);
    expect(FakeEventSource.instances[1]!.url).toContain(Z_B);
    a();
    b();
  });

  it("clears both a pending reconnect timer and idle timer on teardown", () => {
    // Drive the entry into RECONNECTING (schedules a reconnect timer) and then
    // release its only subscriber (schedules an idle timer). Tearing down while
    // BOTH timers are still pending exercises the two clearTimeout guards in
    // teardown — the reconnecting-then-abandoned tab path.
    const clearSpy = vi.spyOn(globalThis, "clearTimeout");
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const es = FakeEventSource.instances[0]!;
    es.onerror?.call(es as unknown as EventSource, {} as Event);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.RECONNECTING);
    a();
    clearSpy.mockClear();
    // __resetRegistryForTests runs teardown directly without advancing timers,
    // so both the reconnect timer and the idle timer are still live.
    __resetRegistryForTests();
    expect(clearSpy).toHaveBeenCalledTimes(2);
    expect(FakeEventSource.instances[0]!.closed).toBe(true);
    clearSpy.mockRestore();
  });

  it("tears down a still-subscribed reconnecting entry, clearing only the reconnect timer", () => {
    // RECONNECTING but with a live subscriber: refCount stays > 0 so no idle
    // timer is scheduled. Teardown must clear the reconnect timer and skip the
    // (null) idle timer — the no-idle-timer side of the teardown guard.
    const clearSpy = vi.spyOn(globalThis, "clearTimeout");
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const es = FakeEventSource.instances[0]!;
    es.onerror?.call(es as unknown as EventSource, {} as Event);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.RECONNECTING);
    clearSpy.mockClear();
    __resetRegistryForTests();
    expect(clearSpy).toHaveBeenCalledTimes(1);
    expect(FakeEventSource.instances[0]!.closed).toBe(true);
    clearSpy.mockRestore();
    a();
  });
});

describe("fleet-stream-registry — optimistic mutations", () => {
  it("appendOptimistic adds a 'optimistic' row and returns a tempId", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "deploy canary", "steer:k@e2e.com");
    expect(tempId).toMatch(/^optim-/);
    const snap = getSnapshot(Z_A);
    expect(snap.events).toHaveLength(1);
    expect(snap.events[0]!.id).toBe(tempId);
    expect(snap.events[0]!.status).toBe("optimistic");
    expect(snap.events[0]!.text).toBe("deploy canary");
    a();
  });

  it("reconcileOptimistic swaps tempId for the real event_id and clears optimistic", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "x", "steer:k@e2e.com");
    reconcileOptimistic(Z_A, tempId, "evt_real");
    const snap = getSnapshot(Z_A);
    expect(snap.events).toHaveLength(1);
    expect(snap.events[0]!.id).toBe("evt_real");
    expect(snap.events[0]!.status).toBe("received");
    a();
  });

  it("markOptimisticFailed flips the matching row to 'failed', keeping its tempId", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "send that fails", "steer:k@e2e.com");
    markOptimisticFailed(Z_A, tempId);
    const snap = getSnapshot(Z_A);
    expect(snap.events).toHaveLength(1);
    expect(snap.events[0]!.id).toBe(tempId);
    expect(snap.events[0]!.status).toBe("failed");
    a();
  });

  it("appendOptimistic with no active subscription is a no-op (returns empty string)", () => {
    const tempId = appendOptimistic("never_subscribed", "x", "actor");
    expect(tempId).toBe("");
    expect(getSnapshot("never_subscribed").events).toHaveLength(0);
  });
});

describe("fleet-stream-registry — mutation edges", () => {
  it("reconcileOptimistic is a no-op for a fleet with no active subscription", () => {
    reconcileOptimistic("never_subscribed", "temp_x", "evt_x");
    expect(getSnapshot("never_subscribed").events).toHaveLength(0);
  });

  it("markOptimisticFailed is a no-op for a fleet with no active subscription", () => {
    markOptimisticFailed("never_subscribed", "temp_x");
    expect(getSnapshot("never_subscribed").events).toHaveLength(0);
  });

  it("rewrites only the matching optimistic row and leaves the others untouched", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const keep = appendOptimistic(Z_A, "first", "steer:k");
    const target = appendOptimistic(Z_A, "second", "steer:k");
    reconcileOptimistic(Z_A, target, "evt_real");
    const snap = getSnapshot(Z_A);
    expect(snap.events.find((e) => e.id === "evt_real")?.status).toBe("received");
    expect(snap.events.find((e) => e.id === keep)?.status).toBe("optimistic");
    a();
  });

  it("markOptimisticFailed touches only the matching row", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const keep = appendOptimistic(Z_A, "first", "steer:k");
    const target = appendOptimistic(Z_A, "second", "steer:k");
    markOptimisticFailed(Z_A, target);
    const snap = getSnapshot(Z_A);
    expect(snap.events.find((e) => e.id === target)?.status).toBe("failed");
    expect(snap.events.find((e) => e.id === keep)?.status).toBe("optimistic");
    a();
  });

  it("calling the returned unsubscribe again after teardown is a no-op", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    a();
    vi.advanceTimersByTime(IDLE_RELEASE_MS + 1);
    expect(() => a()).not.toThrow();
  });
});
