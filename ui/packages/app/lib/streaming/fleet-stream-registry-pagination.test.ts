import { describe, expect, it, vi } from "vitest";
import { getSnapshot, subscribe } from "./fleet-stream-registry";
import { setupRegistryTests, row, WS, Z_A, NO_SEED } from "@/tests/helpers/fleet-stream-registry-fixtures";
import { setupBackfillTests, SEED_AT_MS, MISSED_AT_MS, OUTAGE_NEWEST_MS, OUTAGE_UNREACHABLE_MS, SEED_SINCE_PARAM, fetchSpy, pageWith, descRows, queryOf, flushBackfill, reconnect, reconnectAgain } from "@/tests/helpers/fleet-stream-backfill-fixtures";

setupRegistryTests();
setupBackfillTests();

describe("fleet-stream-registry — pagination", () => {
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
    const url = String(fetchSpy.mock.calls[0]?.[0]);
    expect(url).not.toContain("since=");
    expect(url).not.toContain("cursor=");
    expect(url).toContain("limit=200");
    expect(getSnapshot(Z_A).events.map((e) => e.id)).toEqual(["evt_first_ever"]);
    a();
  });
});
