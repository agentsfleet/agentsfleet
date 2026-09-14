import { describe, expect, it, vi } from "vitest";
import { subscribe } from "./fleet-stream-registry";
import { FRAME_KIND } from "@/lib/api/events-types";
import { setupRegistryTests, row, WS, Z_A, sourceAt } from "@/tests/helpers/fleet-stream-registry-fixtures";
import { setupBackfillTests, RECONNECT_ADVANCE_MS, SEED_AT_MS, MISSED_AT_MS, SEED_SINCE_PARAM, MISSED_SINCE_PARAM, fetchSpy, pageWith, flushBackfill, reconnect, reconnectAgain } from "@/tests/helpers/fleet-stream-backfill-fixtures";

setupRegistryTests();
setupBackfillTests();

describe("fleet-stream-registry — watermark", () => {
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
    expect(String(fetchSpy.mock.calls[1]?.[0])).toContain(
      `since=${encodeURIComponent(MISSED_SINCE_PARAM)}`,
    );
    a();
  });

  it("a live client-stamped frame never advances the since watermark", async () => {
    // Live frames are stamped with the client clock; a skewed clock keying
    // the cursor would push `since` past frames published in the outage.
    fetchSpy.mockResolvedValueOnce(pageWith([]));
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    const es0 = sourceAt(0);
    es0.open();
    es0.heartbeat();
    es0.emit({
      kind: FRAME_KIND.EVENT_RECEIVED,
      event_id: "evt_live",
      actor: "fleet",
    });
    es0.fail();
    vi.advanceTimersByTime(RECONNECT_ADVANCE_MS);
    const es1 = sourceAt(1);
    es1.open();
    es1.heartbeat();
    await flushBackfill();
    expect(String(fetchSpy.mock.calls[0]?.[0])).toContain(
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
    const pendingFetch = Promise.withResolvers<unknown>();
    fetchSpy.mockReturnValueOnce(pendingFetch.promise);
    const a = subscribe(WS, Z_A, [row({ event_id: "evt_seed", created_at: SEED_AT_MS })], () => {});
    const es1 = reconnect();
    reconnectAgain(es1);
    await flushBackfill();
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    pendingFetch.resolve(pageWith([]));
    await flushBackfill();
    a();
  });


});
