import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { listZombieEventsMock } = vi.hoisted(() => ({
  listZombieEventsMock: vi.fn(),
}));

vi.mock("@/lib/api/events", async () => {
  const actual = await vi.importActual<typeof import("@/lib/api/events")>(
    "@/lib/api/events",
  );
  return { ...actual, listZombieEvents: listZombieEventsMock };
});

import {
  __resetRegistryForTests,
  CONNECTION_STATUS,
  appendOptimistic,
  getSnapshot,
  reconcileOptimistic,
  subscribe,
} from "./zombie-stream-registry";

// Mirrors the FakeEventSource pattern in tests/use-zombie-event-stream.test.ts.
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

const WS = "ws_1";
const Z_A = "zomb_a";
const Z_B = "zomb_b";
const TOKEN = "tok_test";
const IDLE_RELEASE_MS = 30_000;

beforeEach(() => {
  vi.useFakeTimers();
  FakeEventSource.instances = [];
  (globalThis as unknown as { EventSource: unknown }).EventSource = FakeEventSource;
  listZombieEventsMock.mockReset();
  listZombieEventsMock.mockResolvedValue({ items: [], next_cursor: null });
  __resetRegistryForTests();
});

afterEach(() => {
  __resetRegistryForTests();
  vi.useRealTimers();
  delete (globalThis as { EventSource?: unknown }).EventSource;
});

describe("zombie-stream-registry — subscribe lifecycle", () => {
  it("opens a single EventSource per zombieId regardless of subscriber count", () => {
    const a = subscribe(WS, Z_A, TOKEN, () => {});
    const b = subscribe(WS, Z_A, TOKEN, () => {});
    expect(FakeEventSource.instances.length).toBe(1);
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
    a();
    b();
  });

  it("notifies every active listener when the snapshot changes", () => {
    const l1 = vi.fn();
    const l2 = vi.fn();
    const a = subscribe(WS, Z_A, TOKEN, l1);
    const b = subscribe(WS, Z_A, TOKEN, l2);
    const es = FakeEventSource.instances[0]!;
    es.onopen?.call(es as unknown as EventSource, {} as Event);
    expect(l1).toHaveBeenCalled();
    expect(l2).toHaveBeenCalled();
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
    a();
    b();
  });
});

describe("zombie-stream-registry — refcount + idle release", () => {
  it("keeps the EventSource alive when one of two subscribers detaches", () => {
    const a = subscribe(WS, Z_A, TOKEN, () => {});
    const b = subscribe(WS, Z_A, TOKEN, () => {});
    a();
    // Refcount still 1 — connection stays open.
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
    b();
  });

  it("starts an idle timer (not an immediate close) when refcount hits zero", () => {
    const a = subscribe(WS, Z_A, TOKEN, () => {});
    a();
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
    // Advance just shy of the idle window — still open.
    vi.advanceTimersByTime(IDLE_RELEASE_MS - 1);
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
  });

  it("tears the EventSource down once the idle window elapses with no resubscribe", () => {
    const a = subscribe(WS, Z_A, TOKEN, () => {});
    a();
    vi.advanceTimersByTime(IDLE_RELEASE_MS + 1);
    expect(FakeEventSource.instances[0]!.closed).toBe(true);
  });

  it("survives a same-zombie revisit within the idle window — no new EventSource", () => {
    // Same-zombie /dashboard ↔ /zombies/[id] round-trip is the load-bearing
    // DX case: the EventSource must NOT reconnect when the user comes back
    // within IDLE_RELEASE_MS.
    const a = subscribe(WS, Z_A, TOKEN, () => {});
    a();
    vi.advanceTimersByTime(IDLE_RELEASE_MS / 2);
    const b = subscribe(WS, Z_A, TOKEN, () => {});
    expect(FakeEventSource.instances.length).toBe(1);
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
    // Advance well past the original idle window — the resubscription
    // cancelled the timer, so the connection still stands.
    vi.advanceTimersByTime(IDLE_RELEASE_MS * 2);
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
    b();
  });

  it("opens a fresh EventSource on cross-zombie subscription", () => {
    const a = subscribe(WS, Z_A, TOKEN, () => {});
    const b = subscribe(WS, Z_B, TOKEN, () => {});
    expect(FakeEventSource.instances.length).toBe(2);
    expect(FakeEventSource.instances[0]!.url).toContain(Z_A);
    expect(FakeEventSource.instances[1]!.url).toContain(Z_B);
    a();
    b();
  });
});

describe("zombie-stream-registry — token presence", () => {
  it("opens SSE without a bearer token (cookie-authed stream) but skips backfill", () => {
    const a = subscribe(WS, Z_A, null, () => {});
    expect(FakeEventSource.instances.length).toBe(1);
    expect(listZombieEventsMock).not.toHaveBeenCalled();
    a();
  });

  it("kicks off the previously-skipped backfill when a later subscriber brings a token", async () => {
    const a = subscribe(WS, Z_A, null, () => {});
    expect(listZombieEventsMock).not.toHaveBeenCalled();
    const b = subscribe(WS, Z_A, TOKEN, () => {});
    // Backfill is fire-and-forget; the call must have been issued
    // synchronously even if the Promise settles later.
    expect(listZombieEventsMock).toHaveBeenCalledTimes(1);
    expect(listZombieEventsMock.mock.calls[0]![2]).toBe(TOKEN);
    a();
    b();
  });
});

describe("zombie-stream-registry — optimistic mutations", () => {
  it("appendOptimistic adds a 'optimistic' row and returns a tempId", () => {
    const a = subscribe(WS, Z_A, TOKEN, () => {});
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
    const a = subscribe(WS, Z_A, TOKEN, () => {});
    const tempId = appendOptimistic(Z_A, "x", "steer:k@e2e.com");
    reconcileOptimistic(Z_A, tempId, "evt_real");
    const snap = getSnapshot(Z_A);
    expect(snap.events).toHaveLength(1);
    expect(snap.events[0]!.id).toBe("evt_real");
    expect(snap.events[0]!.status).toBe("received");
    a();
  });

  it("appendOptimistic with no active subscription is a no-op (returns empty string)", () => {
    const tempId = appendOptimistic("never_subscribed", "x", "actor");
    expect(tempId).toBe("");
    expect(getSnapshot("never_subscribed").events).toHaveLength(0);
  });
});
