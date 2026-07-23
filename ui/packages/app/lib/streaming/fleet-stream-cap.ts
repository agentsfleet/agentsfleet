import type { FleetEvent } from "./fleet-stream-frames";

// The live-view size cap. Extracted from the registry (length cap) — it is a
// pure function with no registry state, so it sits on its own.
//
// A tab left open on a busy fleet accumulates live events without bound: the
// array only ever grows, and every streaming frame regroups the whole of it.
// The live view keeps a bounded window; the full history is one tab away in
// Events, which is paginated.
//
// Only SETTLED rows are trimmable. An earlier version trimmed the oldest row
// outright on the theory that a pending row is always the newest — true at
// append time, but `mergeBackfill` re-sorts the whole array by `createdAt`,
// and optimistic rows carry the CLIENT clock while backfill rows carry the
// SERVER clock. Under clock skew a pending send can sort below fresh backfill
// rows and be trimmed as "oldest"; its reconcile graft then finds nothing and
// the message blanks until reload. Same hazard for a row still streaming its
// reply. So a not-yet-terminal row (optimistic, failed-and-retryable, or
// received/streaming) is never evicted, whatever its position — the cap only
// sheds rows whose result is already final.
export const MAX_LIVE_EVENTS = 200;

// The three terminal statuses a delivery settles into. A row in any other
// state (optimistic, failed-retryable, received/streaming) is live and stays.
const TRIMMABLE_STATUSES: ReadonlySet<string> = new Set([
  "processed",
  "fleet_error",
  "gate_blocked",
]);

export function capEvents(events: FleetEvent[]): FleetEvent[] {
  if (events.length <= MAX_LIVE_EVENTS) return events;
  // Shed the oldest settled rows first (the array is oldest→newest), and only
  // as many as the overflow needs; never a pending or streaming row. If a
  // fleet somehow holds 200+ unsettled rows the window grows rather than drop
  // one — the right trade, since an unsettled row is live state.
  let toDrop = events.length - MAX_LIVE_EVENTS;
  const kept: FleetEvent[] = [];
  for (const event of events) {
    if (toDrop > 0 && TRIMMABLE_STATUSES.has(event.status)) {
      toDrop -= 1;
      continue;
    }
    kept.push(event);
  }
  return kept;
}
