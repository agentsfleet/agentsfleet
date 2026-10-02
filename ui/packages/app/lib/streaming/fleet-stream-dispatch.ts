import { type LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { backfillEntry } from "./fleet-stream-backfill";
import type { Entry } from "./fleet-stream-entry";
import { factsOf } from "./fleet-stream-facts";
import { applyLiveFrame, mergeBackfill } from "./fleet-stream-frames";
import {
  dispatchReplyFrame,
  settleRepliesFromBackfill,
  watchRunningRows,
  type ApplyEvents,
} from "./fleet-stream-reply-registry";
import { TERMINAL_STATUSES } from "./fleet-stream-row";
import { patchSnapshot, patchSpokenFacts, setEvents } from "./fleet-stream-snapshot";
import { advanceInstallStep, installStepFromKind } from "./install-steps";

// Where a live frame goes: held while a send here awaits its 202
// (`HeldTurns`), else onto the thread. The registry owns the entry and the
// EventSource; this owns what a frame does to the entry.

// An entry the registry holds, with the write every reply helper makes and its
// check that this entry still owns the fleet. Both are made once, when the
// entry is adopted, so a frame allocates neither.
export type LiveEntry = Entry & { apply: ApplyEvents; isCurrent: () => boolean };

const RUNNER_ACTIVITY_KINDS: ReadonlySet<string> = new Set([
  FRAME_KIND.CHUNK, FRAME_KIND.TOOL_CALL_STARTED,
  FRAME_KIND.TOOL_CALL_PROGRESS, FRAME_KIND.TOOL_CALL_COMPLETED,
]);

// Reads back what the stream missed. A burst of gap signals during a walk
// costs one more walk after it, not one each.
export function recoverGap(entry: LiveEntry, fleetId: string): void {
  void backfillEntry(entry, fleetId, {
    stillCurrent: entry.isCurrent,
    onPage: (rows) => {
      // A held turn the page settles lands its live frames first, as if never
      // held: the durable row then settles it and keeps the tools and text
      // they carried, which the list itself never has.
      landFrames(entry, fleetId, entry.held.releaseTurns(new Set(rows.map((row) => row.event_id))));
      setEvents(entry, (prev) => mergeBackfill(prev, rows));
      watchRunningRows(entry, rows);
      settleRepliesFromBackfill(entry, fleetId, rows, entry.apply, entry.isCurrent);
    },
  });
}

export function onFrame(entry: LiveEntry, fleetId: string, frame: LiveFrame): void {
  if (entry.held.take(frame, entry.snapshot.events)) return;
  applyFrame(entry, fleetId, frame);
}

/** Frames a hold let go of, onto the thread in the order they arrived. */
export function landFrames(entry: LiveEntry, fleetId: string, frames: readonly LiveFrame[]): void {
  for (const frame of frames) applyFrame(entry, fleetId, frame);
}

function applyFrame(entry: LiveEntry, fleetId: string, frame: LiveFrame): void {
  // The daemon lost frames for this stream — dropped behind a slow reader, or
  // a subscription lost and re-established — so read them back as a reconnect does.
  if (frame.kind === FRAME_KIND.CATCHING_UP) {
    recoverGap(entry, fleetId);
    return;
  }
  // Install frames advance the install step, never the message list. Forking
  // here (rather than inside applyLiveFrame) keeps the chat reducer pure and the
  // two concerns — a long-lived chat timeline vs. a one-shot install beat —
  // independent while sharing the single EventSource the spec mandates.
  const installStep = installStepFromKind(frame.kind);
  if (installStep !== null) {
    patchSnapshot(entry, {
      installStep: advanceInstallStep(entry.snapshot.installStep, installStep),
    });
    return;
  }
  // Best-effort activity can arrive after the report's durable close. Keep
  // every late runner frame from mutating the settled answer or tool history.
  if ("event_id" in frame && RUNNER_ACTIVITY_KINDS.has(frame.kind)
    && entry.snapshot.events.some((event) => event.id === frame.event_id && TERMINAL_STATUSES.has(event.status))) return;
  if (dispatchReplyFrame(entry, fleetId, frame, entry.apply, entry.isCurrent)) return;
  // A completion carries the fleet's status and pending count beside its row;
  // a gate frame carries the count alone and touches no row.
  const facts = factsOf(frame);
  if (frame.kind === FRAME_KIND.GATE_OPENED || frame.kind === FRAME_KIND.GATE_RESOLVED) {
    patchSpokenFacts(entry, facts);
    return;
  }
  setEvents(entry, (prev) => applyLiveFrame(prev, frame), facts);
}
