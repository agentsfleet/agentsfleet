import { describe, expect, it, vi } from "vitest";
import { __resetRegistryForTests, CONNECTION_STATUS, appendOptimistic, getSnapshot, reconcileServerRows, subscribe } from "./fleet-stream-registry";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { setupRegistryTests, row, WS, Z_A, Z_B, NO_SEED, IDLE_RELEASE_MS, sourceAt } from "@/tests/helpers/fleet-stream-registry-fixtures";

setupRegistryTests();

describe("fleet-stream-registry — subscribe lifecycle", () => {
  it("opens a single EventSource per fleetId regardless of subscriber count", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const b = subscribe(WS, Z_A, NO_SEED, () => {});
    expect(FakeEventSource.instances.length).toBe(1);
    expect(sourceAt(0).closed).toBe(false);
    a();
    b();
  });

  it("notifies every active listener when the snapshot changes", () => {
    const l1 = vi.fn();
    const l2 = vi.fn();
    const a = subscribe(WS, Z_A, NO_SEED, l1);
    const b = subscribe(WS, Z_A, NO_SEED, l2);
    const es = sourceAt(0);
    es.open();
    es.heartbeat();
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

  it("caps the live view by shedding the oldest SETTLED rows, never a pending send", () => {
    // A tab left open on a busy fleet must not grow the array without bound,
    // but the cap sheds only settled rows — a pending send is live state.
    const seed = Array.from({ length: 260 }, (_, i) =>
      row({ event_id: `evt_${i}`, status: "processed", created_at: Date.UTC(2026, 4, 15, 12, 0, i) }),
    );
    const release = subscribe(WS, Z_A, seed, () => {});
    const tempId = appendOptimistic(Z_A, "one more", "steer:pending");

    const events = getSnapshot(Z_A).events;
    expect(events.length).toBe(200);
    // Oldest settled dropped, newest settled kept, pending survives.
    expect(events.some((e) => e.id === tempId)).toBe(true);
    expect(events.some((e) => e.id === "evt_259")).toBe(true);
    expect(events.some((e) => e.id === "evt_0")).toBe(false);
    release();
  });

  it("keeps a pending row the mergeBackfill sort buried among older settled rows", () => {
    // The reviewer's skew case: a pending optimistic row carries the client
    // clock; a backfill sorts it BELOW newer server rows. The cap must still
    // refuse to evict it, or its reconcile graft finds nothing and the
    // message blanks until reload.
    const release = subscribe(WS, Z_A, [], () => {});
    // Pending row stamped in the past (client clock behind the server).
    const tempId = appendOptimistic(Z_A, "buried steer", "steer:pending");
    // A backfill of 260 newer server rows re-sorts the pending row down-array.
    const backfill = Array.from({ length: 260 }, (_, i) =>
      row({ event_id: `srv_${i}`, status: "processed", created_at: Date.UTC(2026, 4, 15, 18, 0, i) }),
    );
    reconcileServerRows(Z_A, backfill);

    const events = getSnapshot(Z_A).events;
    // The pending row is not the newest, yet it survives the cap.
    expect(events.some((e) => e.id === tempId)).toBe(true);
    expect(events.length).toBeLessThanOrEqual(200 + 1);
    release();
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
    expect(sourceAt(0).closed).toBe(false);
    b();
  });

  it("starts an idle timer (not an immediate close) when refcount hits zero", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    a();
    expect(sourceAt(0).closed).toBe(false);
    vi.advanceTimersByTime(IDLE_RELEASE_MS - 1);
    expect(sourceAt(0).closed).toBe(false);
  });

  it("tears the EventSource down once the idle window elapses with no resubscribe", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    a();
    vi.advanceTimersByTime(IDLE_RELEASE_MS + 1);
    expect(sourceAt(0).closed).toBe(true);
  });

  it("survives a same-fleet revisit within the idle window — no new EventSource", () => {
    // Same-fleet /dashboard ↔ /fleets/[id] round-trip is the load-bearing
    // DX case: the EventSource must NOT reconnect when the user comes back
    // within IDLE_RELEASE_MS.
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    sourceAt(0).open();
    sourceAt(0).heartbeat();
    a();
    vi.advanceTimersByTime(IDLE_RELEASE_MS / 2);
    const b = subscribe(WS, Z_A, NO_SEED, () => {});
    expect(FakeEventSource.instances.length).toBe(1);
    expect(sourceAt(0).closed).toBe(false);
    for (let heartbeat = 0; heartbeat < 4; heartbeat += 1) {
      vi.advanceTimersByTime(IDLE_RELEASE_MS / 2);
      sourceAt(0).heartbeat();
    }
    expect(sourceAt(0).closed).toBe(false);
    b();
  });

  it("opens a fresh EventSource on cross-fleet subscription", () => {
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const b = subscribe(WS, Z_B, NO_SEED, () => {});
    expect(FakeEventSource.instances.length).toBe(2);
    expect(sourceAt(0).url).toContain(Z_A);
    expect(sourceAt(1).url).toContain(Z_B);
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
    const es = sourceAt(0);
    es.fail();
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.RECONNECTING);
    a();
    clearSpy.mockClear();
    // __resetRegistryForTests runs teardown directly without advancing timers,
    // so both the reconnect timer and the idle timer are still live.
    __resetRegistryForTests();
    expect(clearSpy).toHaveBeenCalledTimes(2);
    expect(sourceAt(0).closed).toBe(true);
    clearSpy.mockRestore();
  });

  it("tears down a still-subscribed reconnecting entry, clearing only the reconnect timer", () => {
    // RECONNECTING but with a live subscriber: refCount stays > 0 so no idle
    // timer is scheduled. Teardown must clear the reconnect timer and skip the
    // (null) idle timer — the no-idle-timer side of the teardown guard.
    const clearSpy = vi.spyOn(globalThis, "clearTimeout");
    const a = subscribe(WS, Z_A, NO_SEED, () => {});
    const es = sourceAt(0);
    es.fail();
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.RECONNECTING);
    clearSpy.mockClear();
    __resetRegistryForTests();
    expect(clearSpy).toHaveBeenCalledTimes(1);
    expect(sourceAt(0).closed).toBe(true);
    clearSpy.mockRestore();
    a();
  });
});
