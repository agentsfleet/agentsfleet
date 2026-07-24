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

/** The two shapes a rendered thread entry takes — a lone row, or a collapsed
 * run. Named so the discriminant is written once, not spelled at each site. */
export const ENTRY_KIND = {
  SINGLE: "single",
  GROUP: "group",
} as const;

export type ThreadEntry =
  | { kind: typeof ENTRY_KIND.SINGLE; key: string; event: FleetEvent }
  | { kind: typeof ENTRY_KIND.GROUP; key: string; events: FleetEvent[] };

/**
 * Only integration activity ever coalesces. An operator's message and the
 * fleet's reply are the conversation the thread exists to show — collapsing
 * either would hide a person's own words, and a failed optimistic send would
 * vanish into a count (Invariant 1).
 */
function groupable(event: FleetEvent): boolean {
  return event.role === "system" && event.reply.trim().length === 0;
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
 * `previous` is the last result, and passing it is a performance guarantee, not
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

  // Accumulate the current run and flush it when the next event breaks it. A
  // single `for…of` pass, so there is no by-index access and no undefined
  // element to guard — the lookahead lives in the accumulator, not the array.
  const flush = (run: FleetEvent[]): void => {
    const lead = run[0];
    if (lead === undefined) return;
    const isGroup = run.length >= MIN_GROUP_RUN;
    const key = isGroup ? `group:${lead.id}` : lead.id;
    const built: ThreadEntry = isGroup
      ? { kind: ENTRY_KIND.GROUP, key, events: run }
      : { kind: ENTRY_KIND.SINGLE, key, event: lead };
    const prior = reusable.get(key);
    entries.push(prior !== undefined && sameEntry(prior, built) ? prior : built);
  };

  let run: FleetEvent[] = [];
  for (const event of events) {
    const lead = run[0];
    // Extend only a groupable run of the same delivery; anything else — a
    // non-groupable lead, a role change, a differing outcome — starts fresh.
    if (lead !== undefined && groupable(lead) && groupable(event) && sameRun(lead, event)) {
      run.push(event);
    } else {
      flush(run);
      run = [event];
    }
  }
  flush(run);
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
  // Compared only for entries at the SAME key, and a key encodes its kind
  // (`group:` prefix vs the bare event id), so both sides are always the same
  // kind — no kind comparison, and thus no dead branch. A single's member
  // list is just its one event; a group's is its run.
  const beforeMembers = entryMembers(before);
  const afterMembers = entryMembers(after);
  return beforeMembers.length === afterMembers.length
    && beforeMembers.every((event, i) => event === afterMembers[i]);
}

function entryMembers(entry: ThreadEntry): readonly FleetEvent[] {
  return entry.kind === ENTRY_KIND.GROUP ? entry.events : [entry.event];
}

/**
 * The earliest and latest timestamps a group spans, for its "11:38–12:03"
 * range. Takes a NON-EMPTY run (a group always has ≥`MIN_GROUP_RUN` members),
 * and reads the extremes rather than the ends, so it does not assume a
 * direction the caller might change. Min/max over the members means there is
 * no empty-array branch to leave forever uncovered.
 */
export function groupSpan(members: FleetEvent[]): { first: Date; last: Date } {
  const times = members.map((member) => member.createdAt.getTime());
  return { first: new Date(Math.min(...times)), last: new Date(Math.max(...times)) };
}
