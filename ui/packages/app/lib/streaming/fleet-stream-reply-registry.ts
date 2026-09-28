import type { ActionResult } from "@/lib/actions/with-token";
import type { EventDetail, EventRow, LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import type { FleetFacts } from "@/lib/events/run-summary";
import { factsOf } from "./fleet-stream-facts";
import { applyLiveFrame, mergeBackfill } from "./fleet-stream-frames";
import { applyFinalReply, applyFinalReplyText, applyReplyDelta, applyReplyRecovery } from "./fleet-stream-reply-frames";
import type { Entry } from "./fleet-stream-entry";
import { AGENTSFLEET_EVENT_STATUS, type FleetEvent } from "./fleet-stream-row";
import { ReplyStreamDecoder } from "./reply-stream-decoder";
import { STREAM_SILENCE_TIMEOUT_MS } from "./stream-recovery-window";

type Apply = (next: (prev: FleetEvent[]) => FleetEvent[], facts: Partial<FleetFacts>) => void;

/** Reads one event's saved detail when a streamed reply needs its final text. */
export type EventDetailReader = (workspaceId: string, fleetId: string, eventId: string) => Promise<ActionResult<EventDetail>>;

// The dashboard installs its Server Action here. Importing it directly would
// pull server-only modules into every bundle that loads the registry.
let readEventDetail: EventDetailReader | null = null;

export function setEventDetailReader(reader: EventDetailReader | null): void {
  readEventDetail = reader;
}

/** How long a running event may go without a frame before its row is read
 * again: the stream's own silence window, so a quiet tool is never taken for a
 * lost ending sooner than a quiet stream would be. */
export const REPLY_STALL_MS = STREAM_SILENCE_TIMEOUT_MS;

/** Owns live decoder lifetimes beside the fleet's one EventSource. */
export function dispatchReplyFrame(
  entry: Entry,
  fleetId: string,
  frame: LiveFrame,
  apply: Apply,
  isCurrent: () => boolean,
): boolean {
  if (frame.kind === FRAME_KIND.EVENT_COMPLETE) entry.replyHeard.delete(frame.event_id);
  else if ("event_id" in frame && typeof frame.event_id === "string") watchReply(entry, fleetId, frame.event_id, apply, isCurrent);
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
            apply((prev) => applyDetail(prev, result.data), {});
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
      closeLivePass(entry, row.event_id);
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
  entry.replyHeard.clear();
}

// A running event the stream stopped telling this tab about — its completion
// dropped while the stream stayed up — would keep its row running and its
// Thought clock ticking. Each frame for the event marks it heard; the stream's
// own traffic, every frame and heartbeat, sweeps for events unheard for
// `REPLY_STALL_MS` and reads each one's saved row once. No timer: a stream that
// falls silent altogether is the recovery window's to reopen. `readNow` reads
// at once as well — a replayed send whose event may have run before this page
// saw it — and the sweep retries a read that failed.
export function watchReply(
  entry: Entry,
  fleetId: string,
  eventId: string,
  apply: Apply,
  isCurrent: () => boolean,
  readNow = false,
): void {
  entry.replyHeard.set(eventId, Date.now());
  if (readNow) void settleFromDetail(entry, fleetId, eventId, apply, isCurrent);
}

/** Reads, once, every running event this tab has not heard from in `REPLY_STALL_MS`. */
export function readStalledReplies(entry: Entry, fleetId: string, apply: Apply, isCurrent: () => boolean): void {
  const nowMs = Date.now();
  for (const [eventId, heardAtMs] of entry.replyHeard) {
    if (nowMs - heardAtMs < REPLY_STALL_MS) continue;
    entry.replyHeard.delete(eventId);
    void settleFromDetail(entry, fleetId, eventId, apply, isCurrent);
  }
}

// One read of an event this tab still shows running. An ended run settles its
// row — status and figures from the row, the answer from its text, its live
// pass closed. A failed read, or a run still going, changes nothing.
async function settleFromDetail(
  entry: Entry,
  fleetId: string,
  eventId: string,
  apply: Apply,
  isCurrent: () => boolean,
): Promise<void> {
  const read = readEventDetail;
  const shown = entry.snapshot.events.find((event) => event.id === eventId);
  if (read === null || shown?.status !== AGENTSFLEET_EVENT_STATUS.RECEIVED) return;
  const result = await read(entry.workspaceId, fleetId, eventId).catch(() => null);
  if (!result?.ok || !isCurrent() || result.data.status === AGENTSFLEET_EVENT_STATUS.RECEIVED) return;
  const detail = result.data;
  closeLivePass(entry, eventId);
  apply((prev) => applyDetail(prev, detail), {});
}

// A saved row over the live one: its status and figures, then its answer.
function applyDetail(prev: FleetEvent[], detail: EventDetail): FleetEvent[] {
  return applyFinalReply(mergeBackfill(prev, [detail]), detail);
}

function closeLivePass(entry: Entry, eventId: string): void {
  entry.replyStreams.get(eventId)?.dispose();
  entry.replyStreams.delete(eventId);
  entry.replyNextSeq.delete(eventId);
}
