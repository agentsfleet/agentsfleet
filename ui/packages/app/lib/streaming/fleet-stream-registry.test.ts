import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  __resetRegistryForTests,
  CONNECTION_STATUS,
  appendOptimistic,
  discardOptimistic,
  getSnapshot,
  markOptimisticFailed,
  reconcileOptimistic,
  reconcileServerRows,
  subscribe,
} from "./fleet-stream-registry";
import { FRAME_KIND, type EventRow } from "@/lib/api/events";

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
    cost_nanos: null,
    failure_label: null,
    failure_detail: null,
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

  it("reconciles a refreshed terminal row with its recorded failure outcome", () => {
    const release = subscribe(WS, Z_A, [
      row({ event_id: "evt_live", status: "received", response_text: null }),
    ], () => {});

    reconcileServerRows(Z_A, [
      row({
        event_id: "evt_live",
        status: "fleet_error",
        response_text: null,
        failure_label: "startup_posture",
      }),
    ]);

    expect(getSnapshot(Z_A).events[0]).toMatchObject({
      id: "evt_live",
      status: "fleet_error",
      outcome: "Failed a startup safety check",
    });
    release();
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
    expect(reconcileOptimistic(Z_A, tempId, "evt_real")).toBe(false);
    const snap = getSnapshot(Z_A);
    expect(snap.events).toHaveLength(1);
    expect(snap.events[0]!.id).toBe("evt_real");
    expect(snap.events[0]!.status).toBe("received");
    a();
  });

  it("grafts the operator's text onto a body-less live row that beat the POST response", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const tempId = appendOptimistic(Z_A, "deploy the canary", "steer:k@e2e.com");
    // The SSE EVENT_RECEIVED for this steer lands before the Server Action
    // resolves — the frame carries no message body, so the live row holds
    // the real event id with an empty trigger.
    const es = FakeEventSource.instances[0]!;
    es.onmessage?.call(es as unknown as EventSource, {
      data: JSON.stringify({
        kind: FRAME_KIND.EVENT_RECEIVED,
        event_id: "evt_early",
        actor: "steer:k@e2e.com",
      }),
    } as MessageEvent);
    expect(reconcileOptimistic(Z_A, tempId, "evt_early")).toBe(false);
    const events = getSnapshot(Z_A).events;
    expect(events).toHaveLength(1);
    expect(events[0]!.id).toBe("evt_early");
    // The optimistic row was the only holder of the operator's message;
    // reconciliation must not blank it out of the thread until reload.
    expect(events[0]!.text).toBe("deploy the canary");
    a();
  });

  it("drops the optimistic duplicate when the real event completed before reconciliation", () => {
    const a = subscribe(
      WS,
      Z_A,
      [row({ event_id: "evt_fast", status: "processed", response_text: "done" })],
      () => {},
    );
    const tempId = appendOptimistic(Z_A, "fast task", "steer:k@e2e.com");
    expect(reconcileOptimistic(Z_A, tempId, "evt_fast")).toBe(true);
    const events = getSnapshot(Z_A).events;
    expect(events).toHaveLength(1);
    expect(events[0]?.id).toBe("evt_fast");
    expect(events[0]?.status).toBe("processed");
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

  it("discardOptimistic removes only the matching row", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const keep = appendOptimistic(Z_A, "first", "steer:k");
    const stale = appendOptimistic(Z_A, "second", "steer:k");
    markOptimisticFailed(Z_A, stale);
    discardOptimistic(Z_A, stale);
    expect(getSnapshot(Z_A).events.map((e) => e.id)).toEqual([keep]);
    a();
  });

  it("discardOptimistic is a no-op for a fleet with no active subscription", () => {
    discardOptimistic("never_subscribed", "temp_x");
    expect(getSnapshot("never_subscribed").events).toHaveLength(0);
  });

  it("a stale tempId from a torn-down entry can never discard a fresh row", () => {
    // A FailedDelivery outlives the stream entry: fail, navigate away past
    // the idle window (entry torn down), come back, send a new message. A
    // per-entry counter would hand the new row the SAME id the failure
    // stored, and retry's discard would remove the operator's newest
    // pending message instead of the stale failed one.
    const first = subscribe(WS, Z_A, NO_SEED, () => {});
    const staleTempId = appendOptimistic(Z_A, "old failed send", "steer:k");
    markOptimisticFailed(Z_A, staleTempId);
    first();
    vi.advanceTimersByTime(IDLE_RELEASE_MS);
    expect(getSnapshot(Z_A).events).toHaveLength(0);

    const second = subscribe(WS, Z_A, NO_SEED, () => {});
    const freshTempId = appendOptimistic(Z_A, "newest message", "steer:k");
    expect(freshTempId).not.toBe(staleTempId);
    discardOptimistic(Z_A, staleTempId);
    const events = getSnapshot(Z_A).events;
    expect(events).toHaveLength(1);
    expect(events[0]!.id).toBe(freshTempId);
    expect(events[0]!.text).toBe("newest message");
    second();
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

describe("fleet-stream-registry — reconnect backfill", () => {
  // The registry backfills through the same-origin proxy on reconnect opens
  // only; these drive error→backoff→reopen against a mocked fetch.
  const RECONNECT_ADVANCE_MS = 2_001; // first-retry backoff (base 1s × 2^1) + 1
  const SEED_AT_MS = Date.UTC(2026, 4, 15, 18, 30, 0);
  // A frame published during the outage — any instant after the seed works.
  const MISSED_AT_MS = SEED_AT_MS + 1_000;
  // Spacing between adjacent rows in a mocked newest-first page.
  const ROW_SPACING_MS = 1_000;
  // Newest row of a multi-page outage burst; MISSED_AT_MS is its oldest.
  const OUTAGE_NEWEST_MS = SEED_AT_MS + 3 * ROW_SPACING_MS;
  // Every row of a budget-exhausting walk stays newer than the anchor.
  const OUTAGE_UNREACHABLE_MS = SEED_AT_MS + 5 * ROW_SPACING_MS;
  // SEED_AT_MS minus the 2s overlap, second-truncated ("since" is 20-char RFC 3339).
  const SEED_SINCE_PARAM = "2026-05-15T18:29:58Z";
  // MISSED_AT_MS minus the same overlap — the watermark after a successful backfill.
  const MISSED_SINCE_PARAM = "2026-05-15T18:29:59Z";

  const fetchSpy = vi.fn();
  const originalFetch = globalThis.fetch;

  beforeEach(() => {
    fetchSpy.mockReset();
    globalThis.fetch = fetchSpy as unknown as typeof fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  function pageWith(
    items: EventRow[],
    nextCursor: string | null = null,
  ): { ok: true; json: () => Promise<unknown> } {
    return { ok: true, json: () => Promise.resolve({ items, next_cursor: nextCursor }) };
  }

  // Rows arrive newest-first, mirroring the upstream `created_at DESC` order.
  function descRows(ids: string[], newestMs: number): EventRow[] {
    return ids.map((id, i) => row({ event_id: id, created_at: newestMs - i * ROW_SPACING_MS }));
  }

  function queryOf(callIndex: number): URLSearchParams {
    return new URL(String(fetchSpy.mock.calls[callIndex]![0]), "http://localhost").searchParams;
  }

  // The backfill path awaits fetch → json → merge per page; drain enough
  // microtask hops for a full BACKFILL_MAX_PAGES cursor walk (fake timers stay
  // untouched — nothing here rides a timer).
  async function flushBackfill(): Promise<void> {
    for (let i = 0; i < 200; i += 1) await Promise.resolve();
  }

  function reconnect(): FakeEventSource {
    const es0 = FakeEventSource.instances[0]!;
    es0.onopen?.call(es0 as unknown as EventSource, {} as Event);
    es0.onerror?.call(es0 as unknown as EventSource, {} as Event);
    vi.advanceTimersByTime(RECONNECT_ADVANCE_MS);
    const es1 = FakeEventSource.instances[1]!;
    es1.onopen?.call(es1 as unknown as EventSource, {} as Event);
    return es1;
  }

  // Drive one more error→reopen cycle off the given (open) EventSource.
  function reconnectAgain(es: FakeEventSource): FakeEventSource {
    es.onerror?.call(es as unknown as EventSource, {} as Event);
    vi.advanceTimersByTime(RECONNECT_ADVANCE_MS);
    const next = FakeEventSource.instances[FakeEventSource.instances.length - 1]!;
    next.onopen?.call(next as unknown as EventSource, {} as Event);
    return next;
  }

  it("test_registry_backfills_on_reconnect — error→reopen issues one backfill keyed off the last-seen event and merges the rows", async () => {
    fetchSpy.mockResolvedValueOnce(
      pageWith([row({ event_id: "evt_missed", created_at: MISSED_AT_MS })]),
    );
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    reconnect();
    await flushBackfill();
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const url = String(fetchSpy.mock.calls[0]![0]);
    expect(url).toContain(`/live/v1/workspaces/${WS}/fleets/${Z_A}/events?`);
    expect(url).toContain(`since=${encodeURIComponent(SEED_SINCE_PARAM)}`);
    expect(url).toContain("limit=200");
    expect(getSnapshot(Z_A).events.map((e) => e.id)).toEqual(["evt_seed", "evt_missed"]);
    a();
  });

  it("keeps the recovery anchor behind a reconciled partial page", async () => {
    fetchSpy.mockResolvedValueOnce(
      pageWith([row({ event_id: "evt_missed", created_at: MISSED_AT_MS })]),
    );
    const release = subscribe(WS, Z_A, [
      row({ event_id: "evt_seed", created_at: SEED_AT_MS }),
    ], () => {});
    reconcileServerRows(Z_A, [
      row({ event_id: "evt_newest", created_at: MISSED_AT_MS + 60_000 }),
    ]);

    reconnect();
    await flushBackfill();

    expect(queryOf(0).get("since")).toBe(SEED_SINCE_PARAM);
    expect(getSnapshot(Z_A).events.map((event) => event.id)).toContain("evt_missed");
    release();
  });

  it("test_registry_initial_open_no_backfill — the first-ever onopen issues no backfill fetch", async () => {
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    const es = FakeEventSource.instances[0]!;
    es.onopen?.call(es as unknown as EventSource, {} as Event);
    await flushBackfill();
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
    a();
  });

  it("backfills when the initial connection failed before its first open", async () => {
    fetchSpy.mockResolvedValueOnce(
      pageWith([row({ event_id: "evt_initial_gap", created_at: MISSED_AT_MS })]),
    );
    const a = subscribe(
      WS,
      Z_A,
      [row({ event_id: "evt_seed", created_at: SEED_AT_MS })],
      () => {},
    );
    const initial = FakeEventSource.instances[0]!;
    initial.onerror?.call(initial as unknown as EventSource, {} as Event);
    vi.advanceTimersByTime(RECONNECT_ADVANCE_MS);
    const recovered = FakeEventSource.instances[1]!;
    recovered.onopen?.call(recovered as unknown as EventSource, {} as Event);
    await flushBackfill();
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(getSnapshot(Z_A).events.map((event) => event.id)).toEqual([
      "evt_seed",
      "evt_initial_gap",
    ]);
    a();
  });

  it("test_registry_backfill_dedupes — a row delivered both live and via backfill appears once", async () => {
    fetchSpy.mockResolvedValueOnce(
      pageWith([
        row({ event_id: "evt_seed", created_at: SEED_AT_MS }),
        row({ event_id: "evt_missed", created_at: MISSED_AT_MS }),
      ]),
    );
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    reconnect();
    await flushBackfill();
    const ids = getSnapshot(Z_A).events.map((e) => e.id);
    expect(ids).toEqual(["evt_seed", "evt_missed"]);
    a();
  });

  it("test_registry_backfill_failure_tolerated — a rejected backfill fetch leaves the timeline intact and the stream LIVE", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    fetchSpy.mockRejectedValueOnce(new Error("network drop mid-backfill"));
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    reconnect();
    await flushBackfill();
    expect(getSnapshot(Z_A).events.map((e) => e.id)).toEqual(["evt_seed"]);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
    expect(warnSpy).toHaveBeenCalled();
    warnSpy.mockRestore();
    a();
  });

  it("test_registry_backfill_failure_tolerated — an HTTP-error backfill response is swallowed the same way", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    fetchSpy.mockResolvedValueOnce({ ok: false, status: 503 });
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    reconnect();
    await flushBackfill();
    expect(getSnapshot(Z_A).events.map((e) => e.id)).toEqual(["evt_seed"]);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
    expect(warnSpy).toHaveBeenCalled();
    warnSpy.mockRestore();
    a();
  });

  it("keys the backfill off the last server event, skipping a newer optimistic row", async () => {
    // A steer sent mid-outage appends an optimistic row with a client-clock
    // timestamp; keying `since` off it would skip frames published earlier
    // in the outage window.
    fetchSpy.mockResolvedValueOnce(pageWith([]));
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    const es0 = FakeEventSource.instances[0]!;
    es0.onopen?.call(es0 as unknown as EventSource, {} as Event);
    es0.onerror?.call(es0 as unknown as EventSource, {} as Event);
    appendOptimistic(Z_A, "sent during the outage", "steer:k@e2e.com");
    vi.advanceTimersByTime(RECONNECT_ADVANCE_MS);
    const es1 = FakeEventSource.instances[1]!;
    es1.onopen?.call(es1 as unknown as EventSource, {} as Event);
    await flushBackfill();
    const url = String(fetchSpy.mock.calls[0]![0]);
    expect(url).toContain(`since=${encodeURIComponent(SEED_SINCE_PARAM)}`);
    a();
  });

  it("keys the backfill off the last server event, skipping a newer failed row", async () => {
    fetchSpy.mockResolvedValueOnce(pageWith([]));
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    const es0 = FakeEventSource.instances[0]!;
    es0.onopen?.call(es0 as unknown as EventSource, {} as Event);
    es0.onerror?.call(es0 as unknown as EventSource, {} as Event);
    const tempId = appendOptimistic(Z_A, "steer that fails mid-outage", "steer:k@e2e.com");
    markOptimisticFailed(Z_A, tempId);
    vi.advanceTimersByTime(RECONNECT_ADVANCE_MS);
    const es1 = FakeEventSource.instances[1]!;
    es1.onopen?.call(es1 as unknown as EventSource, {} as Event);
    await flushBackfill();
    const url = String(fetchSpy.mock.calls[0]![0]);
    expect(url).toContain(`since=${encodeURIComponent(SEED_SINCE_PARAM)}`);
    a();
  });

  it("ignores a malformed backfill body whose items is not an array", async () => {
    fetchSpy.mockResolvedValueOnce({ ok: true, json: () => Promise.resolve({ items: "nope" }) });
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    reconnect();
    await flushBackfill();
    expect(getSnapshot(Z_A).events.map((e) => e.id)).toEqual(["evt_seed"]);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
    a();
  });

  it("drops a backfill page that resolves after the entry was torn down", async () => {
    let resolveFetch!: (value: unknown) => void;
    fetchSpy.mockReturnValueOnce(new Promise((resolve) => { resolveFetch = resolve; }));
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    reconnect();
    a();
    vi.advanceTimersByTime(IDLE_RELEASE_MS + 1);
    resolveFetch(pageWith([row({ event_id: "evt_late", created_at: MISSED_AT_MS })]));
    await flushBackfill();
    // Torn down — the late page must not resurrect a snapshot.
    expect(getSnapshot(Z_A).events).toEqual([]);
  });

  it("advances the since watermark only via successful backfill pages", async () => {
    fetchSpy.mockResolvedValueOnce(
      pageWith([row({ event_id: "evt_missed", created_at: MISSED_AT_MS })]),
    );
    fetchSpy.mockResolvedValueOnce(pageWith([]));
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    const es1 = reconnect();
    await flushBackfill();
    reconnectAgain(es1);
    await flushBackfill();
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(String(fetchSpy.mock.calls[1]![0])).toContain(
      `since=${encodeURIComponent(MISSED_SINCE_PARAM)}`,
    );
    a();
  });

  it("a live client-stamped frame never advances the since watermark", async () => {
    // Live frames are stamped with the client clock; a skewed clock keying
    // the cursor would push `since` past frames published in the outage.
    fetchSpy.mockResolvedValueOnce(pageWith([]));
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    const es0 = FakeEventSource.instances[0]!;
    es0.onopen?.call(es0 as unknown as EventSource, {} as Event);
    es0.onmessage?.call(es0 as unknown as EventSource, {
      data: JSON.stringify({
        kind: FRAME_KIND.EVENT_RECEIVED,
        event_id: "evt_live",
        actor: "fleet",
      }),
    } as MessageEvent);
    es0.onerror?.call(es0 as unknown as EventSource, {} as Event);
    vi.advanceTimersByTime(RECONNECT_ADVANCE_MS);
    const es1 = FakeEventSource.instances[1]!;
    es1.onopen?.call(es1 as unknown as EventSource, {} as Event);
    await flushBackfill();
    expect(String(fetchSpy.mock.calls[0]![0])).toContain(
      `since=${encodeURIComponent(SEED_SINCE_PARAM)}`,
    );
    a();
  });

  it("a failed backfill does not advance the watermark — the next reconnect retries the same window", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    fetchSpy.mockRejectedValueOnce(new Error("network drop"));
    fetchSpy.mockResolvedValueOnce(pageWith([]));
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    const es1 = reconnect();
    await flushBackfill();
    reconnectAgain(es1);
    await flushBackfill();
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    for (const call of fetchSpy.mock.calls) {
      expect(String(call[0])).toContain(`since=${encodeURIComponent(SEED_SINCE_PARAM)}`);
    }
    warnSpy.mockRestore();
    a();
  });

  it("holds a single backfill in flight across overlapping reconnect opens", async () => {
    let resolveFetch!: (value: unknown) => void;
    fetchSpy.mockReturnValueOnce(new Promise((resolve) => { resolveFetch = resolve; }));
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    const es1 = reconnect();
    reconnectAgain(es1);
    await flushBackfill();
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    resolveFetch(pageWith([]));
    await flushBackfill();
    a();
  });

  it("test_registry_backfill_paginates_to_anchor — an outage longer than one page walks next_cursor until a page reaches the anchor", async () => {
    // Page 1 is the NEWEST slice of the window (upstream orders created_at
    // DESC) and is full → next_cursor set. Without following it, evt_oldest
    // (published early in the outage) would be lost in a mid-timeline hole.
    fetchSpy.mockResolvedValueOnce(
      pageWith(descRows(["evt_newest", "evt_mid"], OUTAGE_NEWEST_MS), "cursor_page2"),
    );
    fetchSpy.mockResolvedValueOnce(
      pageWith(descRows(["evt_oldest"], MISSED_AT_MS), null),
    );
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    reconnect();
    await flushBackfill();
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(getSnapshot(Z_A).events.map((e) => e.id)).toEqual([
      "evt_seed",
      "evt_oldest",
      "evt_mid",
      "evt_newest",
    ]);
    a();
  });

  it("test_registry_backfill_page_two_uses_cursor_only — page 1 sends since, page 2 sends cursor (upstream rejects both together)", async () => {
    fetchSpy.mockResolvedValueOnce(
      pageWith(descRows(["evt_a", "evt_b"], OUTAGE_NEWEST_MS), "cursor_page2"),
    );
    fetchSpy.mockResolvedValueOnce(
      pageWith(descRows(["evt_c"], MISSED_AT_MS), null),
    );
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    reconnect();
    await flushBackfill();
    const first = queryOf(0);
    expect(first.get("since")).toBe(SEED_SINCE_PARAM);
    expect(first.get("cursor")).toBeNull();
    const second = queryOf(1);
    expect(second.get("cursor")).toBe("cursor_page2");
    expect(second.get("since")).toBeNull();
    a();
  });

  it("test_registry_backfill_empty_timeline_single_page — no anchor means exactly one page, never pagination", async () => {
    // A full page with a next_cursor would tempt the walk; with no anchor to
    // walk back to, following it would drag in the fleet's whole history.
    fetchSpy.mockResolvedValueOnce(
      pageWith(descRows(["evt_first_ever"], SEED_AT_MS), "cursor_page2"),
    );
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    reconnect();
    await flushBackfill();
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(getSnapshot(Z_A).events.map((e) => e.id)).toEqual(["evt_first_ever"]);
    a();
  });

  it("test_registry_backfill_truncation_surfaced — exhausting the page budget warns rather than claiming a complete recovery", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const MAX_PAGES = 10;
    // Every page full, every page newer than the anchor → the walk never
    // reaches it and the budget runs out.
    for (let i = 0; i < MAX_PAGES; i += 1) {
      fetchSpy.mockResolvedValueOnce(
        pageWith(descRows([`evt_${i}`], OUTAGE_UNREACHABLE_MS), `cursor_${i}`),
      );
    }
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    reconnect();
    await flushBackfill();
    expect(fetchSpy).toHaveBeenCalledTimes(MAX_PAGES);
    expect(warnSpy).toHaveBeenCalledWith(
      "fleet-stream backfill failed",
      `recovery truncated at ${MAX_PAGES} pages`,
    );
    warnSpy.mockRestore();
    a();
  });

  it("test_registry_backfill_midpage_failure_keeps_watermark — a failure on page 2 leaves the watermark at the anchor so the next reconnect retries", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    fetchSpy.mockResolvedValueOnce(
      pageWith(descRows(["evt_newest"], OUTAGE_NEWEST_MS), "cursor_page2"),
    );
    fetchSpy.mockResolvedValueOnce({ ok: false, status: 503 });
    fetchSpy.mockResolvedValueOnce(pageWith([], null));
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    const es1 = reconnect();
    await flushBackfill();
    // Page 1's rows are merged (id-dedupe makes the retry idempotent), but the
    // watermark must NOT have advanced past the unrecovered remainder.
    expect(getSnapshot(Z_A).events.map((e) => e.id)).toContain("evt_newest");
    reconnectAgain(es1);
    await flushBackfill();
    expect(queryOf(2).get("since")).toBe(SEED_SINCE_PARAM);
    warnSpy.mockRestore();
    a();
  });

  it("test_registry_backfill_empty_timeline_requests_recent — a reconnect with no last-seen event fetches the most-recent bounded page", async () => {
    fetchSpy.mockResolvedValueOnce(
      pageWith([row({ event_id: "evt_first_ever", created_at: SEED_AT_MS })]),
    );
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    reconnect();
    await flushBackfill();
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const url = String(fetchSpy.mock.calls[0]![0]);
    expect(url).not.toContain("since=");
    expect(url).not.toContain("cursor=");
    expect(url).toContain("limit=200");
    expect(getSnapshot(Z_A).events.map((e) => e.id)).toEqual(["evt_first_ever"]);
    a();
  });
});

describe("fleet-stream-registry — a lost connection recovers itself", () => {
  function failCurrent(): void {
    const es = FakeEventSource.instances.at(-1)!;
    es.onerror?.call(es as unknown as EventSource, {} as Event);
  }

  // Drives past the fast attempts so the connection is reported as not live.
  function exhaustFastAttempts(): void {
    for (let attempt = 0; attempt < FAST_ATTEMPTS + 1; attempt += 1) {
      failCurrent();
      vi.advanceTimersByTime(FAST_BACKOFF_CAP_MS);
    }
  }

  const FAST_ATTEMPTS = 5;
  const FAST_BACKOFF_CAP_MS = 15_000;
  const OFFLINE_RETRY_MS = 30_000;

  it("escalates an accept-then-close upstream instead of hammering at base delay", () => {
    // An unhealthy upstream that opens then immediately errors must NOT reset
    // the attempt count on open — otherwise it retries at the base delay
    // forever, stampeding the failing server. Only a delivered frame proves
    // health and returns to fast backoff.
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    for (let cycle = 0; cycle < FAST_ATTEMPTS + 1; cycle += 1) {
      const es = FakeEventSource.instances.at(-1)!;
      es.onopen?.call(es as unknown as EventSource, {} as Event);
      es.onerror?.call(es as unknown as EventSource, {} as Event);
      vi.advanceTimersByTime(FAST_BACKOFF_CAP_MS);
    }
    // Despite every cycle reaching onopen, the connection is reported not live.
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);
    release();
  });

  it("returns to fast backoff once a real frame proves the stream healthy", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);

    // Recover, and this time a frame actually arrives.
    vi.advanceTimersByTime(OFFLINE_RETRY_MS);
    const es = FakeEventSource.instances.at(-1)!;
    es.onopen?.call(es as unknown as EventSource, {} as Event);
    es.onmessage?.call(es as unknown as EventSource, {
      data: JSON.stringify({ kind: "event_received", event_id: "e1", actor: "fleet" }),
    } as MessageEvent);
    // A subsequent failure is treated as attempt 1 (fast), not a continuation
    // of the exhausted offline count.
    es.onerror?.call(es as unknown as EventSource, {} as Event);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.RECONNECTING);
    release();
  });

  it("keeps trying on its own once the fast attempts are exhausted", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);

    // No operator action of any kind — only time passing.
    const before = FakeEventSource.instances.length;
    vi.advanceTimersByTime(OFFLINE_RETRY_MS);
    expect(FakeEventSource.instances.length).toBe(before + 1);
    release();
  });

  it("reports not-live without ever abandoning the fleet", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    const opened = FakeEventSource.instances.length;

    // Each unhurried attempt that also fails schedules the next one. The old
    // client stopped after a fixed count and only a button brought it back.
    for (let round = 0; round < 3; round += 1) {
      failCurrent();
      vi.advanceTimersByTime(OFFLINE_RETRY_MS);
    }
    expect(FakeEventSource.instances.length).toBe(opened + 3);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);
    release();
  });

  it("retries immediately when the tab returns or the network comes back", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    const opened = FakeEventSource.instances.length;

    document.dispatchEvent(new Event("visibilitychange"));
    expect(FakeEventSource.instances.length).toBe(opened + 1);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.CONNECTING);
    release();
  });

  it("opens exactly one connection when both recovery signals fire together", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    const opened = FakeEventSource.instances.length;

    document.dispatchEvent(new Event("visibilitychange"));
    window.dispatchEvent(new Event("online"));
    // The second signal finds a connection already in flight and does nothing.
    expect(FakeEventSource.instances.length).toBe(opened + 1);
    release();
  });

  it("does not reconnect for a tab that is still hidden", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    const opened = FakeEventSource.instances.length;

    // `visibilitychange` fires on the way OUT as well as in. Reconnecting for a
    // tab nobody is looking at spends a stream slot on nothing.
    const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    document.dispatchEvent(new Event("visibilitychange"));
    expect(FakeEventSource.instances.length).toBe(opened);

    visibility.mockReturnValue("visible");
    document.dispatchEvent(new Event("visibilitychange"));
    expect(FakeEventSource.instances.length).toBe(opened + 1);
    visibility.mockRestore();
    release();
  });

  it("ignores a recovery signal while a connection is already live", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const opened = FakeEventSource.instances.length;
    window.dispatchEvent(new Event("online"));
    expect(FakeEventSource.instances.length).toBe(opened);
    release();
  });

  it("stops listening for recovery once the fleet's last subscriber is gone", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    release();
    vi.advanceTimersByTime(IDLE_RELEASE_MS);
    const afterTeardown = FakeEventSource.instances.length;

    window.dispatchEvent(new Event("online"));
    document.dispatchEvent(new Event("visibilitychange"));
    expect(FakeEventSource.instances.length).toBe(afterTeardown);
  });
});
