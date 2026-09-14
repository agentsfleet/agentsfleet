import { describe, expect, it, vi } from "vitest";
import { CONNECTION_STATUS, appendOptimistic, getSnapshot, markOptimisticFailed, reconcileServerRows, subscribe } from "./fleet-stream-registry";
import { setupRegistryTests, row, WS, Z_A, IDLE_RELEASE_MS, sourceAt } from "@/tests/helpers/fleet-stream-registry-fixtures";
import { setupBackfillTests, RECONNECT_ADVANCE_MS, SEED_AT_MS, MISSED_AT_MS, SEED_SINCE_PARAM, fetchSpy, pageWith, queryOf, flushBackfill, reconnect } from "@/tests/helpers/fleet-stream-backfill-fixtures";

setupRegistryTests();
setupBackfillTests();

describe("fleet-stream-registry — backfill", () => {
  it("test_registry_backfills_on_reconnect — error→reopen issues one backfill keyed off the last-seen event and merges the rows", async () => {
    fetchSpy.mockResolvedValueOnce(
      pageWith([row({ event_id: "evt_missed", created_at: MISSED_AT_MS })]),
    );
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    reconnect();
    await flushBackfill();
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const url = String(fetchSpy.mock.calls[0]?.[0]);
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
    const es = sourceAt(0);
    es.open();
    es.heartbeat();
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
    const initial = sourceAt(0);
    initial.fail();
    vi.advanceTimersByTime(RECONNECT_ADVANCE_MS);
    const recovered = sourceAt(1);
    recovered.open();
    recovered.heartbeat();
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
    const es0 = sourceAt(0);
    es0.open();
    es0.heartbeat();
    es0.fail();
    appendOptimistic(Z_A, "sent during the outage", "steer:k@e2e.com");
    vi.advanceTimersByTime(RECONNECT_ADVANCE_MS);
    const es1 = sourceAt(1);
    es1.open();
    es1.heartbeat();
    await flushBackfill();
    const url = String(fetchSpy.mock.calls[0]?.[0]);
    expect(url).toContain(`since=${encodeURIComponent(SEED_SINCE_PARAM)}`);
    a();
  });

  it("keys the backfill off the last server event, skipping a newer failed row", async () => {
    fetchSpy.mockResolvedValueOnce(pageWith([]));
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    const es0 = sourceAt(0);
    es0.open();
    es0.heartbeat();
    es0.fail();
    const tempId = appendOptimistic(Z_A, "steer that fails mid-outage", "steer:k@e2e.com");
    markOptimisticFailed(Z_A, tempId);
    vi.advanceTimersByTime(RECONNECT_ADVANCE_MS);
    const es1 = sourceAt(1);
    es1.open();
    es1.heartbeat();
    await flushBackfill();
    const url = String(fetchSpy.mock.calls[0]?.[0]);
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
    const pendingFetch = Promise.withResolvers<unknown>();
    fetchSpy.mockReturnValueOnce(pendingFetch.promise);
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    reconnect();
    a();
    vi.advanceTimersByTime(IDLE_RELEASE_MS + 1);
    pendingFetch.resolve(pageWith([row({ event_id: "evt_late", created_at: MISSED_AT_MS })]));
    await flushBackfill();
    // Torn down — the late page must not resurrect a snapshot.
    expect(getSnapshot(Z_A).events).toEqual([]);
  });


});
