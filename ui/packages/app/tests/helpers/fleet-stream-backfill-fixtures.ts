import type { EventRow } from "@/lib/api/events";
import { afterEach, beforeEach, expect, vi } from "vitest";
import { FakeEventSource } from "./fake-event-source";
import { row, sourceAt } from "./fleet-stream-registry-fixtures";

// Recovery requires heartbeat proof after reconnect; these drive
// error→backoff→reopen→heartbeat against a mocked fetch.
export const RECONNECT_ADVANCE_MS = 2_001; // first-retry backoff (base 1s × 2^1) + 1
export const SEED_AT_MS = Date.UTC(2026, 4, 15, 18, 30, 0);
// A frame published during the outage — any instant after the seed works.
export const MISSED_AT_MS = SEED_AT_MS + 1_000;
// Spacing between adjacent rows in a mocked newest-first page.
const ROW_SPACING_MS = 1_000;
// Newest row of a multi-page outage burst; MISSED_AT_MS is its oldest.
export const OUTAGE_NEWEST_MS = SEED_AT_MS + 3 * ROW_SPACING_MS;
// Every row of a budget-exhausting walk stays newer than the anchor.
export const OUTAGE_UNREACHABLE_MS = SEED_AT_MS + 5 * ROW_SPACING_MS;
// SEED_AT_MS minus the 2s overlap, second-truncated ("since" is 20-char RFC 3339).
export const SEED_SINCE_PARAM = "2026-05-15T18:29:58Z";
// MISSED_AT_MS minus the same overlap — the watermark after a successful backfill.
export const MISSED_SINCE_PARAM = "2026-05-15T18:29:59Z";

export const fetchSpy = vi.fn();
const BACKFILL_MICROTASK_HOPS = 200;

export function setupBackfillTests(): void {
  beforeEach(() => {
    fetchSpy.mockReset();
    vi.stubGlobal("fetch", fetchSpy);
  });
  afterEach(() => vi.unstubAllGlobals());
}

export function pageWith(
  items: EventRow[],
  nextCursor: string | null = null,
): { ok: true; json: () => Promise<unknown> } {
  return { ok: true, json: () => Promise.resolve({ items, next_cursor: nextCursor }) };
}

// Rows arrive newest-first, mirroring the upstream `created_at DESC` order.
export function descRows(ids: string[], newestMs: number): EventRow[] {
  return ids.map((id, i) => row({ event_id: id, created_at: newestMs - i * ROW_SPACING_MS }));
}

export function queryOf(callIndex: number): URLSearchParams {
  return new URL(String(fetchSpy.mock.calls[callIndex]?.[0]), "http://localhost").searchParams;
}

// The backfill path awaits fetch → json → merge per page; drain enough
// microtask hops for a full BACKFILL_MAX_PAGES cursor walk (fake timers stay
// untouched — nothing here rides a timer).
export async function flushBackfill(): Promise<void> {
  for (let i = 0; i < BACKFILL_MICROTASK_HOPS; i += 1) await Promise.resolve();
}

export function reconnect(): FakeEventSource {
  const es0 = sourceAt(0);
  es0.open();
  es0.heartbeat();
  es0.fail();
  vi.advanceTimersByTime(RECONNECT_ADVANCE_MS);
  const es1 = sourceAt(1);
  es1.open();
  es1.heartbeat();
  return es1;
}

// Drive one more error→reopen cycle off the given (open) EventSource.
export function reconnectAgain(es: FakeEventSource): FakeEventSource {
  es.fail();
  // Wait for the scheduled retry and reject a stale closed source.
  vi.advanceTimersToNextTimer();
  const next = sourceAt(-1);
  expect(next).not.toBe(es);
  expect(next.closed).toBe(false);
  next.open();
  next.heartbeat();
  return next;
}
