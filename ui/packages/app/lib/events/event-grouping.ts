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
 *
 * `previous` is the last result, and passing it is a performance contract, not
 * a convenience: a streaming reply regroups the whole thread on every frame,
 * and returning fresh objects for runs that did not change would make every
 * row a new reference, so React re-renders the entire list per chunk. Entries
 * whose members are identical are handed back by reference instead, letting
 * the rows above the active one bail out.
 */
export function groupThreadEvents(
  events: FleetEvent[],
  previous?: readonly ThreadEntry[],
): ThreadEntry[] {
  const entries: ThreadEntry[] = [];
  const reusable = new Map<string, ThreadEntry>();
  for (const entry of previous ?? []) reusable.set(entry.key, entry);

  let index = 0;
  while (index < events.length) {
    const event = events[index];
    if (event === undefined) break;
    const run = runLengthAt(events, index);
    const key = run >= MIN_GROUP_RUN ? `group:${event.id}` : event.id;
    const built: ThreadEntry = run >= MIN_GROUP_RUN
      ? { kind: "group", key, events: events.slice(index, index + run) }
      : { kind: "single", key, event };
    const prior = reusable.get(key);
    entries.push(prior !== undefined && sameEntry(prior, built) ? prior : built);
    index += run >= MIN_GROUP_RUN ? run : 1;
  }
  return entries;
}

// Identity by members, not by deep equality: the events themselves are already
// replaced by reference whenever the stream changes one, so comparing the
// references is both cheaper and exactly as strict.
//
// LOAD-BEARING INVARIANT (enforced in `fleet-stream-registry.ts` /
// `fleet-stream-frames.ts`): a FleetEvent is replaced with a fresh object on
// any change — `applyChunk`/`applyEventComplete` spread `{ ...existing, … }`,
// `mergeBackfill` rebuilds changed rows and keeps unchanged ones by reference.
// This reference comparison is only sound while that holds. An in-place
// mutation of a FleetEvent (e.g. `event.reply += chunk`) would strand a stale
// entry here and freeze a row's render. Keep those merges copy-on-write.
function sameEntry(before: ThreadEntry, after: ThreadEntry): boolean {
  if (before.kind !== after.kind) return false;
  if (before.kind === "single" && after.kind === "single") return before.event === after.event;
  if (before.kind === "group" && after.kind === "group") {
    return before.events.length === after.events.length
      && before.events.every((event, i) => event === after.events[i]);
  }
  return false;
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
