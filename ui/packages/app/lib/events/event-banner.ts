// A fleet that is failing the same way over and over should say so once,
// loudly, instead of once per delivery.
//
// Pure and total, like the grouping beside it: a function of the ordered event
// array and nothing else. It reads only the TAIL of the thread, because the
// question the banner answers is "is this fleet broken right now?" — not "has
// it ever been". A run that has since recovered is history, and history
// belongs in the rows.

import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";
import { failureSentenceFor, guidanceFor } from "@/lib/events/event-summary";

/**
 * Consecutive identical failures before the banner speaks. One failure is an
 * event; two of the same kind in a row is a pattern, and only a pattern earns
 * an interruption above the conversation (dashboard restraint).
 */
export const BANNER_MIN_FAILURES = 2;

// Statuses that settle a delivery. Anything else — a run still working, an
// operator's own optimistic row — is not evidence either way, so it neither
// extends the run nor clears the banner.
const TERMINAL: ReadonlySet<string> = new Set(["processed", "fleet_error", "gate_blocked"]);
const FAILED = "fleet_error";

export type FailureBanner = {
  /** The runner's failure class — the identity the run is counted on. */
  label: string;
  /** Plain-language sentence for that class. */
  sentence: string;
  /** The recorded cause, when the runner named one. */
  detail: string | null;
  /** What the operator can do about it, when the class is actionable. */
  guidance: string | null;
  /** How many consecutive deliveries failed this way. Always ≥ the threshold. */
  count: number;
  /** When the most recent of them landed. */
  lastSeen: Date;
};

/**
 * The banner a thread currently warrants, or null. Null covers every healthy
 * case: no events, nothing terminal yet, a single failure, and — the one that
 * matters most — a fleet that has recovered, because the newest terminal event
 * is then a success and the banner must not outlive it.
 */
export function failureBannerFor(events: FleetEvent[]): FailureBanner | null {
  let count = 0;
  let newest: FleetEvent | null = null;
  let label: string | null = null;

  // Walk from the newest backwards. `toReversed` yields the elements directly,
  // so there is no by-index access and no undefined element to guard.
  for (const event of events.toReversed()) {
    if (!TERMINAL.has(event.status)) continue;
    // The first terminal event walking back decides everything: if it is not
    // a classified failure the fleet is not currently failing, and there is
    // nothing to pin.
    const failed = event.status === FAILED && event.failureLabel !== null;
    if (!failed) break;
    if (label === null) {
      label = event.failureLabel;
      newest = event;
    } else if (event.failureLabel !== label) {
      // A different failure ends this run rather than inflating its count.
      break;
    }
    count += 1;
  }

  if (label === null || newest === null || count < BANNER_MIN_FAILURES) return null;
  return {
    label,
    sentence: failureSentenceFor(label),
    detail: newest.failureDetail,
    guidance: guidanceFor(label),
    count,
    lastSeen: newest.createdAt,
  };
}
