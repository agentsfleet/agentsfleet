import type { ActionResult } from "@/lib/actions/with-token";
import type { EventDetail, EventRow, LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import type { FleetFacts } from "@/lib/events/run-summary";
import { factsOf } from "./fleet-stream-facts";
import { applyLiveFrame, mergeBackfill } from "./fleet-stream-frames";
import { applyFinalReply, applyFinalReplyText, applyReplyDelta, applyReplyGone, applyReplyRecovery } from "./fleet-stream-reply-frames";
import type { Entry } from "./fleet-stream-entry";
import { AGENTSFLEET_EVENT_STATUS, type FleetEvent } from "./fleet-stream-row";
import { ReplyStreamDecoder } from "./reply-stream-decoder";
import { STREAM_SILENCE_TIMEOUT_MS } from "./stream-recovery-window";

/** A write to an entry's rows, with what the same frame said about its fleet. */
export type ApplyEvents = (next: (prev: FleetEvent[]) => FleetEvent[], facts: Partial<FleetFacts>) => void;

type ChunkFrame = Extract<LiveFrame, { kind: typeof FRAME_KIND.CHUNK }>;
type CompleteFrame = Extract<LiveFrame, { kind: typeof FRAME_KIND.EVENT_COMPLETE }>;

/** Reads one event's saved detail when a streamed reply needs its final text. */
export type EventDetailReader = (workspaceId: string, fleetId: string, eventId: string) => Promise<ActionResult<EventDetail>>;

// The chat installs its reader here, so the registry names no transport and a
// test can answer for it.
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
  apply: ApplyEvents,
  isCurrent: () => boolean,
): boolean {
  markHeard(entry, frame);
  if (frame.kind === FRAME_KIND.CHUNK) {
    writeChunk(entry, frame, apply, isCurrent);
    return true;
  }
  if (frame.kind !== FRAME_KIND.EVENT_COMPLETE) return false;
  completeReply(entry, fleetId, frame, apply, isCurrent);
  return true;
}

// A completion ends the watch on its event; any other frame naming an event
// marks it heard now.
function markHeard(entry: Entry, frame: LiveFrame): void {
  if (frame.kind === FRAME_KIND.EVENT_COMPLETE) entry.replyHeard.delete(frame.event_id);
  else if ("event_id" in frame && typeof frame.event_id === "string") entry.replyHeard.set(frame.event_id, Date.now());
}

// One piece of a streamed reply, written through its event's decoder. A piece
// out of sequence closes the live pass; the saved detail supplies the rest.
function writeChunk(entry: Entry, frame: ChunkFrame, apply: ApplyEvents, isCurrent: () => boolean): void {
  if (entry.replyGaps.has(frame.event_id)) return;
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
    return;
  }
  if (decoder === undefined) {
    decoder = new ReplyStreamDecoder((delta) => {
      if (isCurrent()) apply((prev) => applyReplyDelta(prev, frame.event_id, delta), {});
    });
    entry.replyStreams.set(frame.event_id, decoder);
  }
  entry.replyNextSeq.set(frame.event_id, seq + 1);
  decoder.write(frame.text, textKind);
}

// A completion settles its row: at once when no live pass is open, else once
// the open pass's decoder has drained.
function completeReply(
  entry: Entry,
  fleetId: string,
  frame: CompleteFrame,
  apply: ApplyEvents,
  isCurrent: () => boolean,
): void {
  const finalReply = typeof frame.final_reply === "string" ? frame.final_reply : null;
  const settle = () => {
    apply((prev) => {
      const completed = applyLiveFrame(prev, frame);
      return finalReply === null
        ? applyReplyRecovery(completed, frame.event_id, false)
        : applyFinalReplyText(completed, frame.event_id, finalReply);
    }, factsOf(frame));
    if (finalReply === null) {
      // Older publishers and oversized replies still need the saved detail, and
      // a missing final chunk has no later sequence number to expose its gap.
      entry.replyGaps.add(frame.event_id);
      recoverFinalReply(entry, fleetId, frame.event_id, apply, isCurrent);
    } else entry.replyGaps.delete(frame.event_id);
  };
  const decoder = entry.replyStreams.get(frame.event_id);
  if (decoder === undefined) {
    settle();
    return;
  }
  entry.replyStreams.delete(frame.event_id);
  entry.replyNextSeq.delete(frame.event_id);
  // Completion can overtake queued activity. Close this live pass before the
  // decoder's asynchronous finish so a late chunk cannot open another one.
  entry.replyGaps.add(frame.event_id);
  const finished = () => {
    if (isCurrent()) settle();
  };
  void decoder.finish().then(finished, finished);
}

const TRANSIENT_MAX_RETRY_MS = 5_000;
const FINAL_REPLY_RETRY_MS = [100, 300, TRANSIENT_MAX_RETRY_MS] as const;
const PERMANENT_DETAIL_RETRY_MS = 60_000;
// A read answered 404 or 410 names an event that no longer exists: the stall
// watch and final-reply recovery both end there. The other permanent statuses
// can heal — a fresh session, a repaired row — so recovery backs off on them.
const HTTP_STATUS_NOT_FOUND = 404;
const HTTP_STATUS_GONE = 410;
const EVENT_GONE_STATUSES: ReadonlySet<number> = new Set([HTTP_STATUS_NOT_FOUND, HTTP_STATUS_GONE]);
const PERMANENT_DETAIL_STATUSES: ReadonlySet<number> = new Set([400, 401, 403, 422]);

function recoverFinalReply(
  entry: Entry,
  fleetId: string,
  eventId: string,
  apply: ApplyEvents,
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
          if (result.status !== undefined && EVENT_GONE_STATUSES.has(result.status)) {
            apply((prev) => applyReplyGone(prev, eventId), {});
            return;
          }
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
  apply: ApplyEvents,
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
// `REPLY_STALL_MS` and reads each one's saved row, once per silence window,
// until the row ends. No timer: a stream that falls silent altogether is the
// recovery window's to reopen. `readNow` reads at once as well — a replayed
// send whose event may have run before this page saw it.
export function watchReply(
  entry: Entry,
  fleetId: string,
  eventId: string,
  apply: ApplyEvents,
  isCurrent: () => boolean,
  readNow: boolean,
): void {
  entry.replyHeard.set(eventId, Date.now());
  if (readNow) void settleFromDetail(entry, fleetId, eventId, apply, isCurrent);
}

/** Watches every row a server read shows still running that this tab is not
 * watching yet, from now. The first open reads no backfill, so a completion
 * published between the server render and the subscribe is otherwise never
 * heard, and its row would run forever. A row already watched keeps its stamp,
 * so a later read never pushes its stall read back. */
export function watchRunningRows(entry: Entry, rows: readonly EventRow[]): void {
  const nowMs = Date.now();
  for (const row of rows) {
    if (row.status !== AGENTSFLEET_EVENT_STATUS.RECEIVED || entry.replyHeard.has(row.event_id)) continue;
    entry.replyHeard.set(row.event_id, nowMs);
  }
}

/** Reads every running event this tab has not heard from in `REPLY_STALL_MS`,
 * once per silence window. */
export function readStalledReplies(entry: Entry, fleetId: string, apply: ApplyEvents, isCurrent: () => boolean): void {
  const nowMs = Date.now();
  for (const [eventId, heardAtMs] of entry.replyHeard) {
    if (nowMs - heardAtMs < REPLY_STALL_MS) continue;
    // Re-armed before the read: a read that fails, or finds the run still
    // going, is tried again one silence window later, never sooner.
    entry.replyHeard.set(eventId, nowMs);
    void settleFromDetail(entry, fleetId, eventId, apply, isCurrent);
  }
}

// One read of an event this tab still shows running. An ended run settles its
// row — status and figures from the row, the answer from its text, its live
// pass closed — and leaves the watch, as does a row something else settled or
// an event the read found gone. Any other failed read, or a run still going,
// changes nothing: the watch reads it again.
async function settleFromDetail(
  entry: Entry,
  fleetId: string,
  eventId: string,
  apply: ApplyEvents,
  isCurrent: () => boolean,
): Promise<void> {
  const shown = entry.snapshot.events.find((event) => event.id === eventId);
  if (shown?.status !== AGENTSFLEET_EVENT_STATUS.RECEIVED) {
    entry.replyHeard.delete(eventId);
    return;
  }
  const read = readEventDetail;
  if (read === null) return;
  const result = await read(entry.workspaceId, fleetId, eventId).catch(() => null);
  if (!isCurrent()) return;
  if (!result?.ok) {
    // A failed read keeps the row as it is and reads again; a gone event ends
    // the reads and settles the row, since nothing is left to wait for.
    if (result?.status !== undefined && EVENT_GONE_STATUSES.has(result.status)) {
      entry.replyHeard.delete(eventId);
      closeLivePass(entry, eventId);
      apply((prev) => applyReplyGone(prev, eventId), {});
    }
    return;
  }
  if (result.data.status === AGENTSFLEET_EVENT_STATUS.RECEIVED) return;
  const detail = result.data;
  entry.replyHeard.delete(eventId);
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
