import {
  backfillFleetEventsUrl,
  backfillWorkspaceEventsUrl,
  type EventRow,
  type EventsPage,
  type EventsQuery,
} from "@/lib/api/events";
import { maxServerCreatedAt, rfc3339Seconds } from "./fleet-stream-frames";

// The reconnect gap-recovery walk. Split out of the registry's lifecycle file
// so that file stays under the LENGTH GATE and the walk is testable without a
// live Entry — it reaches the registry only through the callbacks below.

// Re-fetches a window behind the anchor because upstream's `since` param is
// second-granular RFC 3339: a frame sharing the truncated second must not be
// skipped. mergeBackfill's id-dedupe absorbs the overlap (RULE KYS boundary).
const BACKFILL_OVERLAP_MS = 2_000;
// The upstream page maximum.
const BACKFILL_PAGE_LIMIT = 200;
// A hung proxy response must not pin the promise across a flapping network;
// the next reconnect retries anyway.
const BACKFILL_TIMEOUT_MS = 10_000;
// Bounds the cursor-follow walk. 10 × 200 rows covers any realistic outage;
// exhausting it is a real truncation and is surfaced, not swallowed.
const BACKFILL_MAX_PAGES = 10;

// A backfill failure is deliberately swallowed (live frames have already
// resumed; the next reconnect retries), so this console line is its only
// surfacing — hence the single-site no-console override.
export function warnBackfillFailure(detail: unknown): void {
  // oxlint-disable-next-line no-console
  console.warn("fleet-stream backfill failed", detail);
}

// `ok: false` means "do not advance the watermark" — the window is unrecovered
// and the next reconnect must retry it from the same anchor.
export type BackfillOutcome = { ok: true; watermark: number | null } | { ok: false };

export type BackfillRequest = {
  workspaceId: string;
  fleetId: string;
  /** Newest server-confirmed created_at, or null for a never-seeded fleet. */
  anchorMs: number | null;
  /** False once the owning entry has been torn down mid-flight. */
  stillCurrent: () => boolean;
  onPage: (rows: EventRow[]) => void;
};

export type WorkspaceBackfillRequest = Omit<BackfillRequest, "fleetId">;
type BackfillQuery = Pick<EventsQuery, "cursor" | "since" | "limit">;
type BackfillWalkRequest = WorkspaceBackfillRequest & {
  pageUrl: (query: BackfillQuery) => string;
};

// One page of the backfill list, or null when the fetch failed (already
// diagnosed). Upstream rejects `cursor` together with `since`, so the caller
// sends exactly one of them.
async function fetchBackfillPage(
  url: string,
): Promise<EventsPage | null> {
  const res = await fetch(url, { signal: AbortSignal.timeout(BACKFILL_TIMEOUT_MS) });
  if (!res.ok) {
    warnBackfillFailure(`HTTP ${res.status}`);
    return null;
  }
  const page = (await res.json()) as EventsPage;
  if (!Array.isArray(page.items)) {
    warnBackfillFailure("malformed page body");
    return null;
  }
  return page;
}

// Rows arrive newest-first (`created_at DESC`), so the last one is the oldest.
function oldestCreatedAt(rows: EventRow[]): number | null {
  const last = rows[rows.length - 1];
  return last === undefined ? null : last.created_at;
}

// Recover the frames published while the EventSource was down. Page 1 is keyed
// `since` the anchor minus the overlap; because upstream returns the NEWEST
// page of that window, a burst longer than one page would otherwise strand the
// oldest missed frames in a permanent mid-timeline hole — so we follow
// `next_cursor` backwards (cursor-only, no `since`: upstream rejects both
// together) until a page's oldest row reaches the anchor.
//
// A never-seeded fleet has no anchor to walk back to, so it takes exactly one
// most-recent page — following the cursor there would drag in all of history.
export async function runBackfill(req: BackfillRequest): Promise<BackfillOutcome> {
  return runBackfillWalk({
    ...req,
    pageUrl: (query) => backfillFleetEventsUrl(req.workspaceId, req.fleetId, query),
  });
}

export async function runWorkspaceBackfill(
  req: WorkspaceBackfillRequest,
): Promise<BackfillOutcome> {
  return runBackfillWalk({
    ...req,
    pageUrl: (query) => backfillWorkspaceEventsUrl(req.workspaceId, query),
  });
}

async function runBackfillWalk(req: BackfillWalkRequest): Promise<BackfillOutcome> {
  const { anchorMs, stillCurrent, onPage, pageUrl } = req;
  const floorMs = anchorMs === null ? null : Math.max(anchorMs - BACKFILL_OVERLAP_MS, 0);
  let watermark = anchorMs;
  let cursor: string | undefined;

  for (let page = 0; page < BACKFILL_MAX_PAGES; page += 1) {
    const query =
      cursor === undefined
        ? {
            since: floorMs === null ? undefined : rfc3339Seconds(floorMs),
            limit: BACKFILL_PAGE_LIMIT,
          }
        : { cursor, limit: BACKFILL_PAGE_LIMIT };

    const body = await fetchBackfillPage(pageUrl(query));
    if (body === null) return { ok: false };
    if (!stillCurrent()) return { ok: false };

    watermark = maxServerCreatedAt(watermark, body.items);
    onPage(body.items);

    const oldest = oldestCreatedAt(body.items);
    if (!body.next_cursor || floorMs === null || oldest === null || oldest <= floorMs) {
      return { ok: true, watermark };
    }
    cursor = body.next_cursor;
  }

  // Budget exhausted with the anchor still below us: the oldest missed frames
  // are genuinely unrecovered. Say so — never present it as a full recovery.
  warnBackfillFailure(`recovery truncated at ${BACKFILL_MAX_PAGES} pages`);
  return { ok: true, watermark };
}
