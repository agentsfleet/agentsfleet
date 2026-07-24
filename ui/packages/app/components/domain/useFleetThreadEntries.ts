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

export const RENDER_KIND_KEY = "renderKind";
export const RENDER_KIND = {
  TRIGGER: "trigger",
  REPLY: "reply",
} as const;

export type FleetThreadEntry =
  | ThreadEntry
  | { kind: "reply"; key: string; event: FleetEvent };

export type FleetThreadEntries = {
  entries: FleetThreadEntry[];
  convertEntry: (entry: FleetThreadEntry) => ThreadMessageLike;
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
  const groupedEntries = useMemo(() => {
    const next = groupThreadEvents(events, previous.current);
    previous.current = next;
    return next;
  }, [events]);
  const entries = useMemo(
    () => groupedEntries.flatMap(expandEntry),
    [groupedEntries],
  );
  const convertEntry = useCallback(
    (entry: FleetThreadEntry): ThreadMessageLike => {
      if (entry.kind === "reply") {
        return withRenderKind(convertEvent(entry.event), RENDER_KIND.REPLY);
      }

      const message =
        entry.kind === ENTRY_KIND.SINGLE
          ? convertEvent(entry.event)
          : groupMessage(entry.events, entry.key, convertEvent);

      return entry.kind === ENTRY_KIND.SINGLE
        && entry.event.role !== "assistant"
        && entry.event.reply.trim().length > 0
        ? withRenderKind(message, RENDER_KIND.TRIGGER)
        : message;
    },
    [convertEvent],
  );
  return { entries, convertEntry };
}

function expandEntry(entry: ThreadEntry): FleetThreadEntry[] {
  if (entry.kind === ENTRY_KIND.GROUP) return [entry];

  const { event } = entry;
  if (event.role === "assistant" || event.reply.trim().length === 0) {
    return [entry];
  }

  return [
    entry,
    {
      kind: "reply",
      key: `${entry.key}:reply`,
      event: {
        ...event,
        id: `${event.id}:reply`,
        role: "assistant",
        actor: "fleet",
        text: event.reply,
        reply: event.reply,
        outcome: "",
        failureLabel: null,
        failureDetail: null,
      },
    },
  ];
}

function withRenderKind(
  message: ThreadMessageLike,
  renderKind: (typeof RENDER_KIND)[keyof typeof RENDER_KIND],
): ThreadMessageLike {
  return {
    ...message,
    metadata: {
      ...message.metadata,
      custom: {
        ...message.metadata?.custom,
        [RENDER_KIND_KEY]: renderKind,
      },
    },
  };
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
