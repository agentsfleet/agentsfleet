// Reading an event's saved row after the stream failed to carry its ending:
// the retry cadence, what counts as gone, and how a read lands on the rows.
// Split from the reply registry so each file stays under the length gate; the
// registry owns the reader and passes it in, so this module holds no state.

import type { ActionResult } from "@/lib/actions/with-token";
import type { EventDetail } from "@/lib/api/events";
import { HTTP_STATUS_NOT_FOUND } from "@/lib/api/errors";
import type { MessageRole } from "@/lib/events/event-summary";
import type { FleetFacts } from "@/lib/events/run-summary";
import type { Entry } from "./fleet-stream-entry";
import type { FleetEvent } from "./fleet-stream-row";
import { mergeBackfill } from "./fleet-stream-frames";
import { applyFinalReply, applyReplyGone } from "./fleet-stream-reply-frames";

/** A write to an entry's rows, with what the same frame said about its fleet. */
export type ApplyEvents = (next: (prev: FleetEvent[]) => FleetEvent[], facts: Partial<FleetFacts>) => void;

/** Reads one event's saved detail when a streamed reply needs its final text. */
export type EventDetailReader = (workspaceId: string, fleetId: string, eventId: string) => Promise<ActionResult<EventDetail>>;

const TRANSIENT_MAX_RETRY_MS = 5_000;
const FINAL_REPLY_RETRY_MS = [100, 300, TRANSIENT_MAX_RETRY_MS] as const;
const PERMANENT_DETAIL_RETRY_MS = 60_000;
// A read answered 404 or 410 names an event that no longer exists. Recovery
// reads only events whose row the stream or a list read already showed, so it
// ends on either; the stall watch asks `isGone`, since its event may still be
// queued. The other permanent statuses can heal — a fresh session, a repaired
// row — so recovery backs off on them.
const HTTP_STATUS_GONE = 410;
const EVENT_GONE_STATUSES: ReadonlySet<number> = new Set([HTTP_STATUS_NOT_FOUND, HTTP_STATUS_GONE]);
const OPERATOR_ROLE: MessageRole = "user";
const PERMANENT_DETAIL_STATUSES: ReadonlySet<number> = new Set([400, 401, 403, 422]);

/** Reads an ended event's saved row until it answers: at a short cadence
 * through transient failures, a minute apart through a stale session, and not
 * at all once the event is gone. One read loop per event at a time. */
export function recoverFinalReply(
  entry: Entry,
  fleetId: string,
  eventId: string,
  apply: ApplyEvents,
  isCurrent: () => boolean,
  read: EventDetailReader | null,
): void {
  if (read === null || !isCurrent()) return;
  if (entry.replyRecoveries.has(eventId)) return;
  entry.replyRecoveries.add(eventId);
  const onRead = (detail: EventDetail) => {
    apply((prev) => applyDetail(prev, detail), {});
    entry.replyGaps.delete(eventId);
  };
  // Keep the draft unavailable once the event is gone.
  const onGone = () => apply((prev) => applyReplyGone(prev, eventId), {});
  void readUntilAnswered(entry, fleetId, eventId, isCurrent, read, onRead, onGone)
    .catch(() => {})
    .finally(() => entry.replyRecoveries.delete(eventId));
}

// The read loop every recovery shares: how soon it asks again, and when it
// stops. A read that throws is tried again, since a transient failure clears
// while the live stream stays up; a stale session or a repairable row is asked
// a minute apart, without request churn; a gone event ends it, told to the
// caller that has something to mark. An update that throws ends it too, and
// the caller contains the rejection.
async function readUntilAnswered(
  entry: Entry,
  fleetId: string,
  eventId: string,
  isCurrent: () => boolean,
  read: EventDetailReader,
  onRead: (detail: EventDetail) => void,
  onGone?: () => void,
): Promise<void> {
  for (let attempt = 0; isCurrent(); attempt += 1) {
    let retryMs: number = FINAL_REPLY_RETRY_MS[attempt] ?? TRANSIENT_MAX_RETRY_MS;
    const result = await read(entry.workspaceId, fleetId, eventId).catch(() => null);
    if (result !== null) {
      if (result.ok) {
        if (isCurrent()) onRead(result.data);
        return;
      }
      if (result.status !== undefined && EVENT_GONE_STATUSES.has(result.status)) {
        onGone?.();
        return;
      }
      if (result.status !== undefined && PERMANENT_DETAIL_STATUSES.has(result.status)) retryMs = PERMANENT_DETAIL_RETRY_MS;
    }
    await new Promise<void>((resolve) => setTimeout(resolve, retryMs));
  }
}

// An event that started and ended while the stream was away arrives as a list
// row, which carries neither the operator's message nor the fleet's answer, and
// no later list read can supply them. Its saved row is read once for them.
export function readMissingBodies(
  entry: Entry,
  fleetId: string,
  eventId: string,
  apply: ApplyEvents,
  isCurrent: () => boolean,
  read: EventDetailReader | null,
): void {
  const shown = entry.snapshot.events.find((event) => event.id === eventId);
  if (shown === undefined) return;
  // Only an operator's steer carries a message; every turn can carry an answer.
  const missingMessage = shown.role === OPERATOR_ROLE && shown.text.length === 0;
  if (!missingMessage && shown.reply.length > 0) return;
  if (read === null || entry.bodyReads.has(eventId)) return;
  entry.bodyReads.add(eventId);
  recoverFinalReply(entry, fleetId, eventId, apply, isCurrent, read);
}

// A 410 is final. A 404 is final only for an event whose row existed.
export function isGone(entry: Entry, eventId: string, status: number | undefined): boolean {
  if (status === HTTP_STATUS_GONE) return true;
  return status === HTTP_STATUS_NOT_FOUND && entry.replyExists.has(eventId);
}

/**
 * Reads a settled event again for its saved trace, when its turn had to close
 * a call it never heard end. One read loop per event, however often its
 * completion repeats, at the final reply's cadence: a read that fails once
 * must not leave the guess standing as "interrupted" when the saved row says
 * the call went fine. The answer is already on screen, so a gone event marks
 * nothing; the loop just ends.
 */
export function refreshSavedTrace(
  entry: Entry,
  fleetId: string,
  eventId: string,
  apply: ApplyEvents,
  isCurrent: () => boolean,
  read: EventDetailReader | null,
): void {
  if (read === null || entry.traceReads.has(eventId)) return;
  entry.traceReads.add(eventId);
  const onRead = (detail: EventDetail) => apply((prev) => mergeBackfill(prev, [detail]), {});
  void readUntilAnswered(entry, fleetId, eventId, isCurrent, read, onRead).catch(() => {});
}

// A saved row over the live one: its status and figures, then its answer.
export function applyDetail(prev: FleetEvent[], detail: EventDetail): FleetEvent[] {
  return applyFinalReply(mergeBackfill(prev, [detail]), detail);
}

/** Ends an event's live pass: its decoder and its expected sequence. */
export function closeLivePass(entry: Entry, eventId: string): void {
  entry.replyStreams.get(eventId)?.dispose();
  entry.replyStreams.delete(eventId);
  entry.replyNextSeq.delete(eventId);
}
