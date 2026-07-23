"use client";

import { useCallback, useMemo, useRef } from "react";
import type { ThreadMessageLike } from "@assistant-ui/react";

import { ENTRY_KIND, groupThreadEvents, type ThreadEntry } from "@/lib/events/event-grouping";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";

// What the thread actually renders: the stream's events with each run of
// identical activity folded into one entry. Kept out of `FleetThread` because
// that file is at its length cap and this is a self-contained derivation —
// events in, render entries out, no component state involved.

/** Custom-metadata key the renderer reads a group's members back off. The
 * count and time span are derived from the members, not carried separately. */
export const GROUP_META = {
  MEMBERS: "groupMembers",
} as const;

export type FleetThreadEntries = {
  entries: ThreadEntry[];
  convertEntry: (entry: ThreadEntry) => ThreadMessageLike;
};

/**
 * Group the ordered event array and expose the runtime's message converter.
 * Memoized on the array identity: the stream hands back a fresh array only
 * when something actually changed, so grouping re-runs exactly then.
 */
export function useFleetThreadEntries(
  events: FleetEvent[],
  convertEvent: (event: FleetEvent) => ThreadMessageLike,
): FleetThreadEntries {
  // The previous result is fed back in so unchanged runs keep their identity
  // across a streaming frame — see `groupThreadEvents` on why that matters.
  const previous = useRef<ThreadEntry[]>([]);
  const entries = useMemo(() => {
    const next = groupThreadEvents(events, previous.current);
    previous.current = next;
    return next;
  }, [events]);
  const convertEntry = useCallback(
    (entry: ThreadEntry): ThreadMessageLike => {
      if (entry.kind === ENTRY_KIND.SINGLE) return convertEvent(entry.event);
      return groupMessage(entry.events, entry.key, convertEvent);
    },
    [convertEvent],
  );
  return { entries, convertEntry };
}

// A group borrows the shape of its newest member — same role, so assistant-ui
// routes it the same way — and carries its members through the custom bag. The
// count and time span are derived from those members at render, not packed
// here, so there is one source of truth for them.
function groupMessage(
  members: FleetEvent[],
  key: string,
  convertEvent: (event: FleetEvent) => ThreadMessageLike,
): ThreadMessageLike {
  // The newest member, typed non-undefined: `reduce` with no seed returns the
  // last element as a `FleetEvent`. The caller only builds a group from a
  // non-empty run, so the empty-array throw is unreachable — and, unlike an
  // index access, it is not a branch that would sit forever uncovered.
  const newest = members.reduce((_, event) => event);
  const base = convertEvent(newest);
  return {
    ...base,
    id: key,
    metadata: {
      ...base.metadata,
      custom: {
        ...base.metadata?.custom,
        [GROUP_META.MEMBERS]: members,
      },
    },
  };
}
