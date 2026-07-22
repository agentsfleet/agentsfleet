// Consecutive identical activity collapses into one expandable row.
//
// Pure and total: a function of the already-ordered event array and nothing
// else. The stream registry keeps sole ownership of order and identity — this
// derives a VIEW of that array and never reorders, dedupes, or drops an event.
// Expanding a group must therefore be able to hand back exactly the events it
// swallowed, in their original order, which is why a group carries them rather
// than a count.
//
// Because it is a pure function of the array, a live frame needs no special
// case: when a matching delivery lands, the next render simply sees a longer
// run and the group's count grows.

import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";

/**
 * The smallest run that earns collapsing. Two identical deliveries already
 * read as a stuck record; one is just an event.
 */
export const MIN_GROUP_RUN = 2;

export type ThreadEntry =
  | { kind: "single"; key: string; event: FleetEvent }
  | { kind: "group"; key: string; events: FleetEvent[] };

/**
 * Only integration activity ever coalesces. An operator's message and the
 * fleet's reply are the conversation the thread exists to show — collapsing
 * either would hide a person's own words, and a failed optimistic send would
 * vanish into a count (Invariant 1).
 */
function groupable(event: FleetEvent): boolean {
  return event.role === "system";
}

/**
 * Whether two deliveries are "the same delivery again": the same actor saying
 * the same thing and ending the same way. The outcome is compared on purpose —
 * it already carries the failure cause, so two failures of the same CLASS but
 * different CAUSE stay separate rows rather than merging into a count that
 * would misreport what happened.
 *
 * Compared field by field rather than through a joined key string. The thread
 * regroups on every streaming frame, so a throwaway string per comparison
 * would allocate on the hot path for no benefit, and the first differing
 * field exits early.
 */
function sameRun(a: FleetEvent, b: FleetEvent): boolean {
  return a.actor === b.actor
    && a.text === b.text
    && a.status === b.status
    && a.outcome === b.outcome;
}

/**
 * Collapse each run of ≥`MIN_GROUP_RUN` identical consecutive activity events
 * into one group entry, leaving everything else untouched and in place.
 */
export function groupThreadEvents(events: FleetEvent[]): ThreadEntry[] {
  const entries: ThreadEntry[] = [];
  let index = 0;
  while (index < events.length) {
    const event = events[index];
    if (event === undefined) break;
    const run = runLengthAt(events, index);
    if (run >= MIN_GROUP_RUN) {
      const members = events.slice(index, index + run);
      entries.push({ kind: "group", key: `group:${event.id}`, events: members });
      index += run;
      continue;
    }
    entries.push({ kind: "single", key: event.id, event });
    index += 1;
  }
  return entries;
}

// How many consecutive events from `start` share its run key. Always ≥1; a
// non-groupable event is a run of exactly itself, which is what breaks a run
// when an operator speaks in the middle of a burst.
function runLengthAt(events: FleetEvent[], start: number): number {
  const first = events[start];
  if (first === undefined || !groupable(first)) return 1;
  let length = 1;
  while (start + length < events.length) {
    const next = events[start + length];
    if (next === undefined || !groupable(next) || !sameRun(first, next)) break;
    length += 1;
  }
  return length;
}

/** The first and last timestamps a group spans, for its "11:38–12:03" range. */
export function groupTimeRange(events: FleetEvent[]): { first: Date; last: Date } | null {
  const first = events[0];
  const last = events[events.length - 1];
  if (first === undefined || last === undefined) return null;
  // The array is newest-last in render order, but a group must read
  // earliest-first regardless of which end holds which, so compare rather
  // than assume a direction the caller might change.
  const a = first.createdAt.getTime();
  const b = last.createdAt.getTime();
  return a <= b ? { first: first.createdAt, last: last.createdAt } : { first: last.createdAt, last: first.createdAt };
}
