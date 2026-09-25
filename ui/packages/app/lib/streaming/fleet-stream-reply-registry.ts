import type { ActionResult } from "@/lib/actions/with-token";
import type { EventDetail, EventRow, LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import type { FleetFacts } from "@/lib/events/run-summary";
import { factsOf } from "./fleet-stream-facts";
import { applyLiveFrame } from "./fleet-stream-frames";
import { applyFinalReply, applyFinalReplyText, applyReplyDelta, applyReplyRecovery } from "./fleet-stream-reply-frames";
import type { Entry } from "./fleet-stream-entry";
import { AGENTSFLEET_EVENT_STATUS, type FleetEvent } from "./fleet-stream-row";
import { ReplyStreamDecoder } from "./reply-stream-decoder";

type Apply = (next: (prev: FleetEvent[]) => FleetEvent[], facts: Partial<FleetFacts>) => void;

/** Reads one event's saved detail when a streamed reply needs its final text. */
export type EventDetailReader = (workspaceId: string, fleetId: string, eventId: string) => Promise<ActionResult<EventDetail>>;

// The dashboard installs its Server Action here. Importing it directly would
// pull server-only modules into every bundle that loads the registry.
let readEventDetail: EventDetailReader | null = null;

export function setEventDetailReader(reader: EventDetailReader | null): void {
  readEventDetail = reader;
}

/** Owns live decoder lifetimes beside the fleet's one EventSource. */
export function dispatchReplyFrame(
  entry: Entry,
  fleetId: string,
  frame: LiveFrame,
  apply: Apply,
  isCurrent: () => boolean,
): boolean {
  if (frame.kind === FRAME_KIND.CHUNK) {
    if (entry.replyGaps.has(frame.event_id)) return true;
    let decoder = entry.replyStreams.get(frame.event_id);
    const seq = frame.stream_seq;
    const textKind = frame.text_kind;
    const expected = entry.replyNextSeq.get(frame.event_id);
    if ((textKind !== "answer" && textKind !== "reasoning")
      || !Number.isSafeInteger(seq) || seq === undefined || seq < 0
      || frame.stream_contiguous !== true
      || (decoder !== undefined && (frame.stream_start === true || seq !== expected))
      || (decoder === undefined && (frame.stream_start !== true || seq !== 0))) {
      decoder?.markGap();
      entry.replyGaps.add(frame.event_id);
      apply((prev) => applyReplyRecovery(prev, frame.event_id, frame.stream_start === true && decoder !== undefined), {});
      return true;
    }
    if (decoder === undefined) {
      decoder = new ReplyStreamDecoder((delta) => {
        if (isCurrent()) apply((prev) => applyReplyDelta(prev, frame.event_id, delta), {});
      });
      entry.replyStreams.set(frame.event_id, decoder);
    }
    entry.replyNextSeq.set(frame.event_id, seq + 1);
    decoder.write(frame.text, textKind);
    return true;
  }
  if (frame.kind !== FRAME_KIND.EVENT_COMPLETE) return false;
  const finalReply = typeof frame.final_reply === "string" ? frame.final_reply : null;
  const settle = (prev: FleetEvent[]) => {
    const completed = applyLiveFrame(prev, frame);
    return finalReply === null
      ? applyReplyRecovery(completed, frame.event_id, false)
      : applyFinalReplyText(completed, frame.event_id, finalReply);
  };
  const decoder = entry.replyStreams.get(frame.event_id);
  if (decoder === undefined) {
    apply(settle, factsOf(frame));
    if (finalReply === null) {
      // Older publishers and oversized replies still need the saved detail.
      entry.replyGaps.add(frame.event_id);
      recoverFinalReply(entry, fleetId, frame.event_id, apply, isCurrent);
    } else entry.replyGaps.delete(frame.event_id);
    return true;
  }
  entry.replyStreams.delete(frame.event_id);
  entry.replyNextSeq.delete(frame.event_id);
  // Completion can overtake queued activity. Close this live pass before the
  // decoder's asynchronous finish so a late chunk cannot open another one.
  entry.replyGaps.add(frame.event_id);
  const finished = () => {
    if (!isCurrent()) return;
    apply(settle, factsOf(frame));
    if (finalReply === null) {
      // A missing final chunk has no later sequence number to expose its gap.
      entry.replyGaps.add(frame.event_id);
      recoverFinalReply(entry, fleetId, frame.event_id, apply, isCurrent);
    } else entry.replyGaps.delete(frame.event_id);
  };
  void decoder.finish().then(finished, finished);
  return true;
}

const TRANSIENT_MAX_RETRY_MS = 5_000;
const FINAL_REPLY_RETRY_MS = [100, 300, TRANSIENT_MAX_RETRY_MS] as const;
const PERMANENT_DETAIL_RETRY_MS = 60_000;
const PERMANENT_DETAIL_STATUSES: ReadonlySet<number> = new Set([400, 401, 403, 404, 410, 422]);

function recoverFinalReply(
  entry: Entry,
  fleetId: string,
  eventId: string,
  apply: Apply,
  isCurrent: () => boolean,
): void {
  const read = readEventDetail;
  if (read === null || !isCurrent()) return;
  if (entry.replyRecoveries.has(eventId)) return;
  entry.replyRecoveries.add(eventId);
  void (async () => {
    try {
      for (let attempt = 0; isCurrent(); attempt += 1) {
        let retryMs: number = FINAL_REPLY_RETRY_MS[attempt] ?? TRANSIENT_MAX_RETRY_MS;
        try {
          const result = await read(entry.workspaceId, fleetId, eventId);
          if (result.ok) {
            if (!isCurrent()) return;
            apply((prev) => applyFinalReply(prev, result.data), {});
            entry.replyGaps.delete(eventId);
            return;
          }
          // A stale session or missing event cannot heal at the transient
          // retry cadence. Keep the draft unavailable, but check again after
          // reauthentication or eventual detail repair without request churn.
          if (result.status !== undefined && PERMANENT_DETAIL_STATUSES.has(result.status)) {
            retryMs = PERMANENT_DETAIL_RETRY_MS;
          }
        } catch {
          // A transient detail failure can clear while the live stream stays up.
        }
        await new Promise<void>((resolve) => setTimeout(resolve, retryMs));
      }
    } finally {
      entry.replyRecoveries.delete(eventId);
    }
  })();
}

export function markReplyGap(entry: Entry): void {
  for (const [eventId, decoder] of entry.replyStreams) {
    decoder.markGap();
    entry.replyGaps.add(eventId);
  }
}

/** A terminal history row may be the only closing notice after a stream gap. */
export function settleRepliesFromBackfill(
  entry: Entry,
  fleetId: string,
  rows: EventRow[],
  apply: Apply,
  isCurrent: () => boolean,
): void {
  for (const row of rows) {
    if (row.status === AGENTSFLEET_EVENT_STATUS.RECEIVED) continue;
    if (entry.replyGaps.has(row.event_id)) {
      entry.replyStreams.get(row.event_id)?.dispose();
      entry.replyStreams.delete(row.event_id);
      entry.replyNextSeq.delete(row.event_id);
      apply((prev) => applyReplyRecovery(prev, row.event_id, false), {});
      recoverFinalReply(entry, fleetId, row.event_id, apply, isCurrent);
      continue;
    }
    const decoder = entry.replyStreams.get(row.event_id);
    if (decoder === undefined) continue;
    entry.replyStreams.delete(row.event_id);
    entry.replyNextSeq.delete(row.event_id);
    void decoder.finish().then(() => {
      if (!isCurrent()) return;
      entry.replyGaps.add(row.event_id);
      apply((prev) => applyReplyRecovery(prev, row.event_id, false), {});
      recoverFinalReply(entry, fleetId, row.event_id, apply, isCurrent);
    });
  }
}

export function disposeReplyStreams(entry: Entry): void {
  for (const decoder of entry.replyStreams.values()) decoder.dispose();
  entry.replyStreams.clear();
  entry.replyNextSeq.clear();
  entry.replyGaps.clear();
  entry.replyRecoveries.clear();
}
