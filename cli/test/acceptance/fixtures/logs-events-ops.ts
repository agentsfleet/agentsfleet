/**
 * Owned fixture for the `logs-events-live` slice — a bounded cursor walk
 * over the `events <fleet_id> --json` paginator.
 *
 * The shared fixtures (cli.js, teardown.ts, lifecycle.ts) stay untouched;
 * the cursor-walk loop and its envelope shape live here because no other
 * spec drives pagination. RULE UFS: every wire literal that crosses the
 * CLI boundary or repeats is a named const.
 *
 * Safety contract the walk enforces:
 *   - bounded page count (never loops forever, even if the server returns
 *     a self-referential / non-advancing cursor)
 *   - cursor monotonicity: a `next_cursor` echoed verbatim from the cursor
 *     we just sent is a non-advancing paginator and aborts the walk
 *   - per-page exit-0 + parseable `{items, next_cursor}` envelope
 */

import { runFleetctl } from "./cli.js";

type Env = Readonly<Record<string, string>>;

export const EVENTS_COMMAND = "events" as const;
export const LOGS_COMMAND = "logs" as const;
export const JSON_FLAG = "--json" as const;
export const AGENT_FLAG = "--fleet" as const;
export const LIMIT_FLAG = "--limit" as const;
export const CURSOR_FLAG = "--cursor" as const;

export const ITEMS_KEY = "items" as const;
export const NEXT_CURSOR_KEY = "next_cursor" as const;

// Hard ceiling on pages walked. The server's per-page `--limit` is small
// (see PAGE_LIMIT), so a healthy fixture exhausts well under this bound;
// hitting it means the paginator never returned a null `next_cursor` and
// is treated as a failure by the caller.
export const MAX_PAGES = 25;

// Small page size so even a lightly-seeded fleet produces >1 page when it
// has history, exercising the cursor hand-off. Stays inside the CLI's
// EVENTS_LIMIT_BOUNDS (1..500).
export const PAGE_LIMIT = 5;

export interface EventItem {
  readonly created_at?: number | string | null;
  readonly status?: string | null;
  readonly actor?: string | null;
  readonly [key: string]: unknown;
}

export interface EventsEnvelope {
  readonly items: ReadonlyArray<EventItem>;
  readonly nextCursor: string | null;
  readonly raw: Record<string, unknown>;
}

export interface CursorWalkResult {
  readonly pages: number;
  readonly totalItems: number;
  readonly cursors: ReadonlyArray<string>;
  readonly exhausted: boolean;
}

export type PageVisitor = (envelope: EventsEnvelope, pageIndex: number) => void;

/**
 * Fetch one `events` page and normalise it into `{items, nextCursor}`.
 * Throws on non-zero exit or unparseable stdout — the caller's per-page
 * assertions stay terse.
 */
export async function fetchEventsPage(
  env: Env,
  fleetId: string,
  cursor: string | null,
): Promise<EventsEnvelope> {
  const args = [EVENTS_COMMAND, fleetId, LIMIT_FLAG, String(PAGE_LIMIT)];
  if (cursor) args.push(CURSOR_FLAG, cursor);
  args.push(JSON_FLAG);
  const result = await runFleetctl(args, { env });
  if (result.code !== 0) {
    throw new Error(`events page exited ${result.code}: ${result.stderr.trim() || result.stdout.trim()}`);
  }
  const raw = JSON.parse(result.stdout.trim() || "{}") as Record<string, unknown>;
  const rawItems = raw[ITEMS_KEY];
  const items: EventItem[] = Array.isArray(rawItems) ? (rawItems as EventItem[]) : [];
  const rawCursor = raw[NEXT_CURSOR_KEY];
  const nextCursor = typeof rawCursor === "string" && rawCursor.length > 0 ? rawCursor : null;
  return { items, nextCursor, raw };
}

/**
 * Walk the events cursor from the first page until the server stops
 * returning a `next_cursor`, the page cap is hit, or a non-advancing
 * cursor is detected. Calls `visit` on every page so the caller can run
 * per-page assertions (exit-0 is already guaranteed by `fetchEventsPage`).
 */
export async function walkEventsCursor(
  env: Env,
  fleetId: string,
  visit?: PageVisitor,
): Promise<CursorWalkResult> {
  const seen = new Set<string>();
  const cursors: string[] = [];
  let cursor: string | null = null;
  let totalItems = 0;
  let pages = 0;
  let exhausted = false;

  for (pages = 0; pages < MAX_PAGES; pages += 1) {
    const page = await fetchEventsPage(env, fleetId, cursor);
    if (visit) visit(page, pages);
    totalItems += page.items.length;
    const next = page.nextCursor;
    if (!next) {
      exhausted = true;
      break;
    }
    if (seen.has(next) || next === cursor) {
      throw new Error(`non-advancing cursor at page ${pages}: server re-emitted ${next}`);
    }
    seen.add(next);
    cursors.push(next);
    cursor = next;
  }

  return { pages: pages + (exhausted ? 1 : 0), totalItems, cursors, exhausted };
}
