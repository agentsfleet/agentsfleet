import type { TurnRow, LiveFrame } from "@/lib/api/turns";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { factsOf } from "./fleet-stream-facts";
import {
  applyDetail,
  closeLivePass,
  isGone,
  readMissingBodies,
  recoverFinalReply,
  refreshSavedTrace,
  type ApplyTurns,
  type TurnDetailReader,
} from "./fleet-stream-detail-recovery";
import { applyLiveFrame, mergeBackfill } from "./fleet-stream-frames";
import { applyFinalReplyText, applyReplyDelta, applyReplyGone, applyReplyRecovery } from "./fleet-stream-reply-frames";
import type { Entry } from "./fleet-stream-entry";
import { AGENTSFLEET_TURN_STATUS, TERMINAL_STATUSES } from "./fleet-stream-row";
import { closedAnyAtSettle } from "./fleet-stream-tool-trace";
import { ReplyStream } from "./reply-stream";
import { sendMarks } from "./send-marks";
import { STREAM_SILENCE_TIMEOUT_MS } from "./stream-recovery-window";

type ChunkFrame = Extract<LiveFrame, { kind: typeof FRAME_KIND.CHUNK }>;
type CompleteFrame = Extract<LiveFrame, { kind: typeof FRAME_KIND.TURN_COMPLETE }>;

export type { ApplyTurns, TurnDetailReader } from "./fleet-stream-detail-recovery";

// The chat installs its reader here, so the registry names no transport and a
// test can answer for it.
let readTurnDetail: TurnDetailReader | null = null;

export function setTurnDetailReader(reader: TurnDetailReader | null): void {
  readTurnDetail = reader;
}

/** How long a running turn may go without a frame before its row is read
 * again: the stream's own silence window, so a quiet tool is never taken for a
 * lost ending sooner than a quiet stream would be. */
export const REPLY_STALL_MS = STREAM_SILENCE_TIMEOUT_MS;

/** Owns live decoder lifetimes beside the fleet's one EventSource. */
export function dispatchReplyFrame(
  entry: Entry,
  fleetId: string,
  frame: LiveFrame,
  apply: ApplyTurns,
  isCurrent: () => boolean,
): boolean {
  sendMarks.frame(frame);
  markHeard(entry, frame);
  if (frame.kind === FRAME_KIND.CHUNK) {
    writeChunk(entry, frame, apply, isCurrent);
    return true;
  }
  if (frame.kind !== FRAME_KIND.TURN_COMPLETE) return false;
  completeReply(entry, fleetId, frame, apply, isCurrent);
  return true;
}

// A completion ends the watch on its turn; any other frame naming a turn
// marks it heard now.
function markHeard(entry: Entry, frame: LiveFrame): void {
  if (frame.kind === FRAME_KIND.TURN_COMPLETE) endWatch(entry, frame.turn_id);
  else if ("turn_id" in frame && typeof frame.turn_id === "string") {
    // The daemon publishes a turn's frames only after writing its row.
    entry.replyHeard.set(frame.turn_id, Date.now());
    entry.replyExists.add(frame.turn_id);
  }
}

function endWatch(entry: Entry, turnId: string): void {
  entry.replyHeard.delete(turnId);
  entry.replyExists.delete(turnId);
}

// One piece of a streamed reply, written through its turn's decoder. A piece
// out of sequence closes the live pass; the saved detail supplies the rest.
function writeChunk(entry: Entry, frame: ChunkFrame, apply: ApplyTurns, isCurrent: () => boolean): void {
  if (entry.replyGaps.has(frame.turn_id)) return;
  let decoder = entry.replyStreams.get(frame.turn_id);
  const alreadyOpen = decoder !== undefined;
  if (decoder === undefined) {
    const attempt = entry.replyAttempts.get(frame.turn_id);
    decoder = new ReplyStream((delta) => {
      if (isCurrent() && entry.replyStreams.get(frame.turn_id) === decoder
        && entry.replyAttempts.get(frame.turn_id) === attempt) apply((prev) => applyReplyDelta(prev, frame.turn_id, delta), {});
    });
    entry.replyStreams.set(frame.turn_id, decoder);
  }
  if (!decoder.write({sequence: frame.stream_seq, start: frame.stream_start === true,
    contiguous: frame.stream_contiguous === true, kind: frame.text_kind, text: frame.text})) {
    entry.replyGaps.add(frame.turn_id);
    apply((prev) => applyReplyRecovery(prev, frame.turn_id, frame.stream_start === true && alreadyOpen), {});
  }
}

// A completion settles its row: at once when no live pass is open, else once
// the open pass's decoder has drained.
function completeReply(
  entry: Entry,
  fleetId: string,
  frame: CompleteFrame,
  apply: ApplyTurns,
  isCurrent: () => boolean,
): void {
  entry.completedReplies.add(frame.turn_id);
  const finalReply = typeof frame.final_reply === "string" ? frame.final_reply : null;
  const settle = () => {
    // Set inside `apply`, which runs its update at once (`setTurns`).
    const seen = { closedCalls: false };
    apply((prev) => {
      const completed = applyLiveFrame(prev, frame);
      seen.closedCalls = closedAnyAtSettle(completed, frame.turn_id);
      return finalReply === null
        ? applyReplyRecovery(completed, frame.turn_id, false)
        : applyFinalReplyText(completed, frame.turn_id, finalReply);
    }, factsOf(frame));
    if (finalReply === null) {
      // Older publishers and oversized replies still need the saved detail, and
      // a missing final chunk has no later sequence number to expose its gap.
      entry.replyGaps.add(frame.turn_id);
      recoverFinalReply(entry, fleetId, frame.turn_id, apply, isCurrent, readTurnDetail);
      return;
    }
    entry.replyGaps.delete(frame.turn_id);
    // The answer came inline, but a call the turn closed itself may have ended
    // otherwise: the saved trace says how. A message the frames carried only
    // the head of is whole on the same saved row.
    if (seen.closedCalls || entry.snapshot.turns.some(turn => turn.id === frame.turn_id && (turn.runRestarted || turn.textTrimmed))) refreshSavedTrace(entry, fleetId, frame.turn_id, apply, isCurrent, readTurnDetail);
  };
  const decoder = entry.replyStreams.get(frame.turn_id);
  if (decoder === undefined) {
    settle();
    return;
  }
  // Completion can overtake queued activity. Close this live pass before the
  // decoder's asynchronous finish so a late chunk cannot open another one.
  entry.replyGaps.add(frame.turn_id);
  const finished = () => {
    if (entry.replyStreams.get(frame.turn_id) !== decoder) return;
    entry.replyStreams.delete(frame.turn_id);
    if (isCurrent()) settle();
  };
  void decoder.finish().then(finished, finished);
}


export function markReplyGap(entry: Entry): void {
  for (const [turnId, decoder] of entry.replyStreams) {
    decoder.markGap();
    entry.replyGaps.add(turnId);
  }
}

/** A terminal history row may be the only closing notice after a stream gap. */
export function settleRepliesFromBackfill(
  entry: Entry,
  fleetId: string,
  rows: TurnRow[],
  apply: ApplyTurns,
  isCurrent: () => boolean,
): void {
  for (const row of rows) {
    if (!TERMINAL_STATUSES.has(row.status)) continue;
    if (entry.replyGaps.has(row.turn_id)) {
      closeLivePass(entry, row.turn_id);
      apply((prev) => applyReplyRecovery(prev, row.turn_id, false), {});
      recoverFinalReply(entry, fleetId, row.turn_id, apply, isCurrent, readTurnDetail);
      continue;
    }
    const decoder = entry.replyStreams.get(row.turn_id);
    if (decoder === undefined) {
      readMissingBodies(entry, fleetId, row.turn_id, apply, isCurrent, readTurnDetail);
      continue;
    }
    entry.completedReplies.add(row.turn_id);
    void decoder.finish().then(() => {
      if (entry.replyStreams.get(row.turn_id) !== decoder) return;
      entry.replyStreams.delete(row.turn_id);
      if (!isCurrent()) return;
      entry.replyGaps.add(row.turn_id);
      apply((prev) => applyReplyRecovery(prev, row.turn_id, false), {});
      recoverFinalReply(entry, fleetId, row.turn_id, apply, isCurrent, readTurnDetail);
    });
  }
}

export function disposeReplyStreams(entry: Entry): void {
  for (const decoder of entry.replyStreams.values()) decoder.dispose();
  entry.replyStreams.clear();
  entry.replyAttempts.clear();
  entry.completedReplies.clear();
  entry.replyGaps.clear();
  entry.replyRecoveries.clear();
  entry.replyHeard.clear();
  entry.replyExists.clear();
  entry.runningReads.clear();
  entry.bodyReads.clear();
  entry.traceReads.clear();
}

// A running turn the stream stopped telling this tab about — its completion
// dropped while the stream stayed up — would keep its row running and its
// Thought clock ticking. Each frame for the turn marks it heard; the stream's
// own traffic, every frame and heartbeat, sweeps for turns unheard for
// `REPLY_STALL_MS` and reads each one's saved row, once per silence window,
// until the row ends. No timer: a stream that falls silent altogether is the
// recovery window's to reopen. `readNow` reads at once as well — a replayed
// send whose turn may have run before this page saw it.
export function watchReply(
  entry: Entry,
  fleetId: string,
  turnId: string,
  apply: ApplyTurns,
  isCurrent: () => boolean,
  readNow: boolean,
): void {
  entry.replyHeard.set(turnId, Date.now());
  if (readNow) void settleFromDetail(entry, fleetId, turnId, apply, isCurrent);
}

/** Watches every row a server read shows still running that this tab is not
 * watching yet, from now. The first open reads no backfill, so a completion
 * published between the server render and the subscribe is otherwise never
 * heard, and its row would run forever. A row already watched keeps its stamp,
 * so a later read never pushes its stall read back. */
export function watchRunningRows(entry: Entry, rows: readonly TurnRow[]): void {
  const nowMs = Date.now();
  for (const row of rows) {
    if (row.status !== AGENTSFLEET_TURN_STATUS.RECEIVED) continue;
    // A server read showed it running, so its row exists.
    entry.replyExists.add(row.turn_id);
    if (!entry.replyHeard.has(row.turn_id)) entry.replyHeard.set(row.turn_id, nowMs);
  }
}

/** Reads every running turn this tab has not heard from in `REPLY_STALL_MS`,
 * once per silence window. */
export function readStalledReplies(entry: Entry, fleetId: string, apply: ApplyTurns, isCurrent: () => boolean): void {
  const nowMs = Date.now();
  for (const [turnId, heardAtMs] of entry.replyHeard) {
    if (nowMs - heardAtMs < REPLY_STALL_MS) continue;
    // Re-armed before the read: a read that fails, or finds the run still
    // going, is tried again one silence window later, never sooner.
    entry.replyHeard.set(turnId, nowMs);
    void settleFromDetail(entry, fleetId, turnId, apply, isCurrent);
  }
}

/** A running turn can predate the list watermark. Read its persisted progress
 * after each coalesced gap recovery, even when new frames keep it from stalling.
 * One read at a time bounds the gap's detail traffic. */
export async function recoverRunningReplies(entry: Entry, fleetId: string, apply: ApplyTurns, isCurrent: () => boolean): Promise<void> {
  const ids = entry.snapshot.turns.filter((turn) => turn.status === AGENTSFLEET_TURN_STATUS.RECEIVED).map((turn) => turn.id);
  for (const turnId of ids) {
    if (!isCurrent()) return;
    // An older silence read may predate this gap. Wait for it, then read the
    // newly persisted progress under the same coalesced recovery owner.
    await entry.runningReads.get(turnId);
    await settleFromDetail(entry, fleetId, turnId, apply, isCurrent);
  }
}

// One read of a turn this tab still shows running. An ended run settles its
// row — status and figures from the row, the answer from its text, its live
// pass closed — and leaves the watch, as does a row something else settled or
// a turn the read found gone. Any other failed read, or a run still going,
// changes nothing: the watch reads it again. A 404 for a turn whose row was
// never known to exist is one still queued — a replayed
// send read before a runner took it — and is read again like any other miss.
async function settleFromDetail(
  entry: Entry,
  fleetId: string,
  turnId: string,
  apply: ApplyTurns,
  isCurrent: () => boolean,
): Promise<void> {
  if (!isCurrent()) return;
  const pending = entry.runningReads.get(turnId);
  if (pending !== undefined) return pending;
  const attempt = entry.replyAttempts.get(turnId);
  const owns = () => isCurrent() && entry.replyAttempts.get(turnId) === attempt;
  const reading = readRunningDetail(entry, fleetId, turnId, apply, owns);
  entry.runningReads.set(turnId, reading);
  try {
    await reading;
  } finally {
    if (entry.runningReads.get(turnId) === reading) entry.runningReads.delete(turnId);
  }
}

async function readRunningDetail(entry: Entry, fleetId: string, turnId: string, apply: ApplyTurns, isCurrent: () => boolean): Promise<void> {
  const shown = entry.snapshot.turns.find((turn) => turn.id === turnId);
  if (shown?.status !== AGENTSFLEET_TURN_STATUS.RECEIVED) {
    endWatch(entry, turnId);
    return;
  }
  const read = readTurnDetail;
  if (read === null) return;
  const result = await read(entry.workspaceId, fleetId, turnId).catch(() => null);
  if (!isCurrent()) return;
  if (!result?.ok) {
    // A failed read keeps the row as it is and reads again; a gone turn ends
    // the reads and settles the row, since nothing is left to wait for.
    if (isGone(entry, turnId, result?.status)) {
      endWatch(entry, turnId);
      closeLivePass(entry, turnId);
      apply((prev) => applyReplyGone(prev, turnId), {});
    }
    return;
  }
  if (result.data.status === AGENTSFLEET_TURN_STATUS.RECEIVED) {
    entry.replyExists.add(turnId);
    apply((prev) => mergeBackfill(prev, [result.data]), {});
    return;
  }
  const detail = result.data;
  endWatch(entry, turnId);
  closeLivePass(entry, turnId);
  apply((prev) => applyDetail(prev, detail), {});
}

