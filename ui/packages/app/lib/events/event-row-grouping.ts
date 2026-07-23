// The Events table stops reading like a stuck record.
//
// Only consecutive FAILURES collapse. Two successes that happen to share an
// actor and status are still two different pieces of work — their results
// differ, and merging them would hide what the fleet actually did. A repeated
// failure is the opposite: the same delivery failing the same way, where the
// fifteenth row tells the operator nothing the first did not.
//
// Grouping is page-local by construction: it is a function of the rows the
// server returned for the cursor in the URL, so a group can never claim more
// than the page it can see (keyset pagination gives no way to know what the
// previous page ended with).

import type { EventRow } from "@/lib/api/events";

/** The status a runner report writes for a failed execution. */
const STATUS_FAILED = "fleet_error";

/** Consecutive identical failures before the table collapses them. */
export const MIN_ROW_GROUP = 2;

export type EventRowEntry = {
  /** The row that stands for the entry — the first of a run, in page order. */
  lead: EventRow;
  /** Every row the entry covers, lead included. Length 1 when it stands alone. */
  rows: EventRow[];
};

/**
 * Two rows are the same failure repeating when the same actor failed the same
 * check for the same reason. The cause is part of the identity on purpose: one
 * class with two different causes is two problems, and a single count would
 * misreport both.
 */
function sameFailure(a: EventRow, b: EventRow): boolean {
  return a.status === STATUS_FAILED
    && b.status === STATUS_FAILED
    && a.actor === b.actor
    && a.failure_label === b.failure_label
    && a.failure_detail === b.failure_detail
    // A classified failure is required: rows with no label are unexplained,
    // and counting them together would claim a shared cause nobody recorded.
    && a.failure_label !== null;
}

/** Collapse each run of ≥`MIN_ROW_GROUP` identical consecutive failures. */
export function groupEventRows(rows: EventRow[]): EventRowEntry[] {
  const entries: EventRowEntry[] = [];
  // Accumulate a run and flush it when the next row breaks it — a single
  // `for…of` pass, so no by-index access and no undefined element to guard.
  const flush = (run: EventRow[]): void => {
    const lead = run[0];
    if (lead !== undefined) entries.push({ lead, rows: run });
  };
  let run: EventRow[] = [];
  for (const row of rows) {
    const lead = run[0];
    if (lead !== undefined && sameFailure(lead, row)) {
      run.push(row);
    } else {
      flush(run);
      run = [row];
    }
  }
  flush(run);
  return entries;
}

/** A metric worth dimming: a failed run's zero is absence, not a measurement. */
export function isZeroMetricOnFailure(row: EventRow, value: number | null): boolean {
  return row.status === STATUS_FAILED && value === 0;
}
