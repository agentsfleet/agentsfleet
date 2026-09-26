"use client";

import { useCallback, useMemo, useRef } from "react";
import type { ThreadMessageLike } from "@assistant-ui/react";

import { ENTRY_KIND, groupThreadEvents, type ThreadEntry } from "@/lib/events/event-grouping";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { toReplyMessage } from "./fleetReplyMessage";

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
} as const;

/** A reply row's id is its trigger's id with this suffix. */
export const REPLY_ID_SUFFIX = ":reply";

const SPLIT = {
  TRIGGER: "trigger",
  REPLY: "reply",
} as const;

export type FleetThreadEntry =
  | ThreadEntry
  | { kind: typeof SPLIT.TRIGGER; key: string; event: FleetEvent }
  | { kind: typeof SPLIT.REPLY; key: string; event: FleetEvent };

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
      switch (entry.kind) {
        case SPLIT.TRIGGER:
          return withRenderKind(convertEvent(entry.event), RENDER_KIND.TRIGGER);
        case SPLIT.REPLY:
          return toReplyMessage(convertEvent(entry.event), entry.event);
        case ENTRY_KIND.GROUP:
          return groupMessage(entry.events, entry.key, convertEvent);
        default:
          // A row the fleet itself wrote is already its own reply.
          return entry.event.role === "assistant"
            ? toReplyMessage(convertEvent(entry.event), entry.event)
            : convertEvent(entry.event);
      }
    },
    [convertEvent],
  );
  return { entries, convertEntry };
}

function expandEntry(entry: ThreadEntry): FleetThreadEntry[] {
  if (entry.kind === ENTRY_KIND.GROUP || !hasReplyRow(entry.event)) return [entry];
  const { event } = entry;
  return [
    { kind: SPLIT.TRIGGER, key: entry.key, event },
    {
      kind: SPLIT.REPLY,
      key: `${entry.key}${REPLY_ID_SUFFIX}`,
      event: {
        ...event,
        id: `${event.id}${REPLY_ID_SUFFIX}`,
        role: "assistant",
        actor: "fleet",
        text: event.reply,
      },
    },
  ];
}

// An operator turn always answers in its own row, so the wait state and the
// reply's parts sit on one assistant message from the first frame — no remount
// when the first word lands. An integration turn earns a reply row only once
// the fleet produced something; until then its tick carries the state.
function hasReplyRow(event: FleetEvent): boolean {
  if (event.role === "assistant") return false;
  if (event.role === "user") return true;
  return event.reply.trim().length > 0
    || (event.reasoning?.length ?? 0) > 0
    || (event.tools?.length ?? 0) > 0;
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
