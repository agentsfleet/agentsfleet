import { describe, expect, it, vi } from "vitest";

import { runBackfill } from "./fleet-stream-backfill";
import { getSnapshot, subscribe } from "./fleet-stream-registry";
import { row, setupRegistryTests, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";
import {
  descRows,
  fetchSpy,
  flushBackfill,
  MISSED_AT_MS,
  OUTAGE_NEWEST_MS,
  pageWith,
  reconnect,
  SEED_AT_MS,
  setupBackfillTests,
} from "@/tests/helpers/fleet-stream-backfill-fixtures";

setupRegistryTests();
setupBackfillTests();

const PAGE_TWO = "cursor_page2";
const PAGE_THREE = "cursor_page3";
// Between the newest page and the anchor, so the walk needs all three pages.
const MIDDLE_PAGE_MS = OUTAGE_NEWEST_MS - 1_000;
const UNAVAILABLE = 503;
const SEED = "evt_seed";
const NEWEST = "evt_p1";
const MIDDLE = "evt_p2";
const OLDEST = "evt_p3";
const NETWORK_DOWN = "network down";

function threePages() {
  fetchSpy.mockResolvedValueOnce(pageWith(descRows([NEWEST], OUTAGE_NEWEST_MS), PAGE_TWO));
  fetchSpy.mockResolvedValueOnce(pageWith(descRows([MIDDLE], MIDDLE_PAGE_MS), PAGE_THREE));
  fetchSpy.mockResolvedValueOnce(pageWith(descRows([OLDEST], MISSED_AT_MS), null));
}

function walk(stillCurrent: () => boolean = () => true) {
  const onPage = vi.fn();
  const outcome = runBackfill({ workspaceId: WS, fleetId: Z_A, anchorMs: SEED_AT_MS, stillCurrent, onPage });
  return { onPage, outcome };
}

describe("the recovery walk hands its rows over once", () => {
  it("test_backfill_walk_notifies_once: a three-page walk merges every page and notifies its thread once", async () => {
    threePages();
    const listener = vi.fn();
    const release = subscribe(WS, Z_A, [row({ event_id: SEED, created_at: SEED_AT_MS })], listener);
    reconnect();
    const heardBefore = listener.mock.calls.length;
    await flushBackfill();
    expect(fetchSpy).toHaveBeenCalledTimes(3);
    expect(listener.mock.calls.length - heardBefore).toBe(1);
    expect(getSnapshot(Z_A).events.map((event) => event.id)).toEqual([SEED, OLDEST, MIDDLE, NEWEST]);
    release();
  });

  it("hands every page to its owner in one call", async () => {
    threePages();
    const { onPage, outcome } = walk();
    await expect(outcome).resolves.toEqual({ ok: true, watermark: OUTAGE_NEWEST_MS });
    expect(onPage).toHaveBeenCalledTimes(1);
    expect(onPage.mock.calls[0]?.[0].map((r: { event_id: string }) => r.event_id)).toEqual([NEWEST, MIDDLE, OLDEST]);
  });

  it("still lands the rows before a page that failed, and leaves the window unrecovered", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    fetchSpy.mockResolvedValueOnce(pageWith(descRows([NEWEST], OUTAGE_NEWEST_MS), PAGE_TWO));
    fetchSpy.mockResolvedValueOnce({ ok: false, status: UNAVAILABLE });
    const { onPage, outcome } = walk();
    await expect(outcome).resolves.toEqual({ ok: false });
    expect(onPage).toHaveBeenCalledTimes(1);
    warn.mockRestore();
  });

  it("still lands the rows before a page whose fetch threw", async () => {
    fetchSpy.mockResolvedValueOnce(pageWith(descRows([NEWEST], OUTAGE_NEWEST_MS), PAGE_TWO));
    fetchSpy.mockRejectedValueOnce(new Error(NETWORK_DOWN));
    const { onPage, outcome } = walk();
    await expect(outcome).rejects.toThrow(NETWORK_DOWN);
    expect(onPage).toHaveBeenCalledTimes(1);
  });

  it("lands nothing once its owner is gone", async () => {
    threePages();
    let pages = 0;
    const { onPage, outcome } = walk(() => {
      pages += 1;
      return pages < 2;
    });
    await expect(outcome).resolves.toEqual({ ok: false });
    expect(onPage).not.toHaveBeenCalled();
  });

  it("hands nothing over for an empty window", async () => {
    fetchSpy.mockResolvedValueOnce(pageWith([], null));
    const { onPage, outcome } = walk();
    await expect(outcome).resolves.toEqual({ ok: true, watermark: SEED_AT_MS });
    expect(onPage).not.toHaveBeenCalled();
  });
});
