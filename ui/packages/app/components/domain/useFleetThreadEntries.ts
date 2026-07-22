"use client";

import { useCallback, useMemo, useRef } from "react";
import type { ThreadMessageLike } from "@assistant-ui/react";

import { groupThreadEvents, groupTimeRange, type ThreadEntry } from "@/lib/events/event-grouping";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";

// What the thread actually renders: the stream's events with each run of
// identical activity folded into one entry. Kept out of `FleetThread` because
// that file is at its length cap and this is a self-contained derivation —
// events in, render entries out, no component state involved.

/** Custom-metadata keys the renderer reads off a grouped message. */
export const GROUP_META = {
  COUNT: "groupCount",
  MEMBERS: "groupMembers",
  FIRST_AT: "groupFirstAt",
  LAST_AT: "groupLastAt",
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
      if (entry.kind === "single") return convertEvent(entry.event);
      return groupMessage(entry.events, entry.key, convertEvent);
    },
    [convertEvent],
  );
  return { entries, convertEntry };
}

// A group borrows the shape of its newest member — same actor, headline and
// outcome by construction — and adds the count, the span, and the members the
// row hands back when it expands.
function groupMessage(
  members: FleetEvent[],
  key: string,
  convertEvent: (event: FleetEvent) => ThreadMessageLike,
): ThreadMessageLike {
  const newest = members[members.length - 1];
  // Unreachable in practice: `groupThreadEvents` never emits an empty group.
  // Falling back beats asserting — a render must not throw over presentation.
  if (newest === undefined) return { role: "system", id: key, content: [] };
  const base = convertEvent(newest);
  const range = groupTimeRange(members);
  return {
    ...base,
    id: key,
    metadata: {
      ...base.metadata,
      custom: {
        ...base.metadata?.custom,
        [GROUP_META.COUNT]: members.length,
        [GROUP_META.MEMBERS]: members,
        [GROUP_META.FIRST_AT]: range?.first,
        [GROUP_META.LAST_AT]: range?.last,
      },
    },
  };
}
