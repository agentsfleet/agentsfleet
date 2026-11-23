"use client";

import { useCallback, useMemo, useRef } from "react";
import type { ThreadMessageLike } from "@assistant-ui/react";

import { ENTRY_KIND, groupThreadTurns, type ThreadEntry } from "@/lib/turns/turn-grouping";
import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";
import { toReplyMessage } from "../../activity/replies/fleetReplyMessage";

// What the thread actually renders: the stream's turns with each run of
// identical activity folded into one entry. Kept out of `FleetThread` because
// that file is at its length cap and this is a self-contained derivation —
// turns in, render entries out, no component state involved.

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

type TriggerEntry = { kind: typeof SPLIT.TRIGGER; key: string; turn: FleetTurn };

export type FleetThreadEntry =
  | ThreadEntry
  | TriggerEntry
  | { kind: typeof SPLIT.REPLY; key: string; turn: FleetTurn; requestActor: string };

export type FleetThreadEntries = {
  entries: FleetThreadEntry[];
  convertEntry: (entry: FleetThreadEntry) => ThreadMessageLike;
};

/**
 * Group the ordered turn array and expose the runtime's message converter.
 * Memoized on the array identity: the stream hands back a fresh array only
 * when something actually changed, so grouping re-runs exactly then.
 */
export function useFleetThreadEntries(
  turns: FleetTurn[],
  convertTurn: (turn: FleetTurn) => ThreadMessageLike,
  breakBefore?: ReadonlySet<string>,
): FleetThreadEntries {
  // The previous result is fed back in so unchanged runs keep their identity
  // across a streaming frame — see `groupThreadTurns` on why that matters.
  const previous = useRef<ThreadEntry[]>([]);
  const groupedEntries = useMemo(() => {
    const next = groupThreadTurns(turns, previous.current, breakBefore);
    previous.current = next;
    return next;
  }, [turns, breakBefore]);
  // The trigger each split turn rendered last, by key: a streamed reply
  // replaces its turn, and the trigger keeps its object while `convertTurn`
  // renders it the same.
  const triggers = useRef(new Map<string, TriggerEntry>());
  const entries = useMemo(() => {
    const held = triggers.current;
    const next = new Map<string, TriggerEntry>();
    const expanded = groupedEntries.flatMap((entry) => expandedOnce(entry, held, convertTurn));
    for (const entry of expanded) if (entry.kind === SPLIT.TRIGGER) next.set(entry.key, entry);
    triggers.current = next;
    return expanded;
  }, [groupedEntries, convertTurn]);
  const convertEntry = useCallback(
    (entry: FleetThreadEntry): ThreadMessageLike => {
      switch (entry.kind) {
        case SPLIT.TRIGGER:
          return withRenderKind(convertTurn(entry.turn), RENDER_KIND.TRIGGER);
        case SPLIT.REPLY:
          return toReplyMessage(convertTurn(entry.turn), entry.turn, entry.requestActor);
        case ENTRY_KIND.GROUP:
          return groupMessage(entry.turns, entry.key, convertTurn);
        default:
          // A row the fleet itself wrote is already its own reply.
          return entry.turn.role === "assistant"
            ? toReplyMessage(convertTurn(entry.turn), entry.turn)
            : convertTurn(entry.turn);
      }
    },
    [convertTurn],
  );
  return { entries, convertEntry };
}

// The expansion of each grouped entry, kept for as long as that entry object
// lives. `groupThreadTurns` hands back an unchanged entry by reference, and
// assistant-ui caches each converted message by the object it was handed
// (core `thread-message-converter.ts`); building fresh trigger and reply
// wrappers per frame would re-convert every settled turn on every chunk. Keyed
// weakly, so an entry the stream replaced takes its expansion with it.
const EXPANDED = new WeakMap<ThreadEntry, FleetThreadEntry[]>();

function expandedOnce(
  entry: ThreadEntry,
  triggers: ReadonlyMap<string, TriggerEntry>,
  convertTurn: (turn: FleetTurn) => ThreadMessageLike,
): FleetThreadEntry[] {
  const held = EXPANDED.get(entry);
  if (held !== undefined) return held;
  const expanded = expandEntry(entry, triggers.get(entry.key), convertTurn);
  EXPANDED.set(entry, expanded);
  return expanded;
}

function expandEntry(
  entry: ThreadEntry,
  previous: TriggerEntry | undefined,
  convertTurn: (turn: FleetTurn) => ThreadMessageLike,
): FleetThreadEntry[] {
  if (entry.kind === ENTRY_KIND.GROUP || !hasReplyRow(entry.turn)) return [entry];
  const { turn } = entry;
  // Compared on what `convertTurn` renders, never on a list of the fields it
  // reads: a field it starts reading is compared from that day on.
  const trigger = previous !== undefined && sameValues(convertTurn(previous.turn), convertTurn(turn))
    ? previous
    : { kind: SPLIT.TRIGGER, key: entry.key, turn };
  return [
    trigger,
    {
      kind: SPLIT.REPLY,
      key: `${entry.key}${REPLY_ID_SUFFIX}`,
      requestActor: turn.actor,
      turn: {
        ...turn,
        id: `${turn.id}${REPLY_ID_SUFFIX}`,
        role: "assistant",
        actor: "fleet",
        text: turn.reply,
      },
    },
  ];
}

/** Whether two converted messages hold the same values: dates by instant,
 * arrays and objects by their entries, anything else by `Object.is`. */
export function sameValues(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (a instanceof Date || b instanceof Date) {
    return a instanceof Date && b instanceof Date && a.getTime() === b.getTime();
  }
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  const left = Object.entries(a);
  const right = new Map(Object.entries(b));
  return left.length === right.size && left.every(([key, value]) => right.has(key) && sameValues(value, right.get(key)));
}

// An operator turn always answers in its own row, so the wait state and the
// reply's parts sit on one assistant message from the first frame — no remount
// when the first word lands. An integration turn earns a reply row only once
// the fleet produced something or restarted; its reply then carries the state.
function hasReplyRow(turn: FleetTurn): boolean {
  if (turn.role === "assistant") return false;
  if (turn.role === "user") return true;
  return turn.runRestarted === true || turn.reply.trim().length > 0
    || (turn.reasoning?.length ?? 0) > 0
    || (turn.tools?.length ?? 0) > 0;
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
  members: FleetTurn[],
  key: string,
  convertTurn: (turn: FleetTurn) => ThreadMessageLike,
): ThreadMessageLike {
  // The newest member, typed non-undefined: `reduce` with no seed returns the
  // last element as a `FleetTurn`. The caller only builds a group from a
  // non-empty run, so the empty-array throw is unreachable — and, unlike an
  // index access, it is not a branch that would sit forever uncovered.
  const newest = members.reduce((_, turn) => turn);
  const base = convertTurn(newest);
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
