import { mergeTaskActivity, readTaskActivityHistory } from "../task-activity";
import type { TurnDetail } from "@/lib/api/turns";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { applyLiveFrame } from "./fleet-stream-frames";
import { OUTCOME } from "@/lib/turns/turn-summary";
import { AGENTSFLEET_TURN_STATUS, closeReasoningSpan, rowToTurn, type FleetTurn } from "./fleet-stream-row";
import type { ReplyDelta } from "./reply-stream-decoder";

/** Fold already classified text; model protocol never enters the row. */
export function applyReplyDelta(
  prev: FleetTurn[],
  turnId: string,
  delta: ReplyDelta,
  nowMs: number = Date.now(),
): FleetTurn[] {
  const index = prev.findIndex((turn) => turn.id === turnId);
  if (index < 0) {
    const opened = applyLiveFrame(prev, { text_kind: "answer", kind: FRAME_KIND.CHUNK, turn_id: turnId, text: delta.answer }, nowMs);
    return opened.map((turn) => turn.id === turnId
      ? stampReasoningSpan({ ...turn, reasoning: delta.reasoning, thinking: delta.thinking }, delta, nowMs)
      : turn);
  }
  return prev.map((turn, at) => at === index ? stampReasoningSpan({
    ...turn,
    reply: turn.reply + delta.answer,
    reasoning: (turn.reasoning ?? "") + delta.reasoning,
    thinking: delta.thinking,
    contextProgressHidden: turn.contextProgress ? true : undefined,
  }, delta, nowMs) : turn);
}

/** Mark a streamed draft as awaiting its durable ending. */
export function applyReplyRecovery(
  prev: FleetTurn[],
  turnId: string,
  clearDraft: boolean,
  nowMs: number = Date.now(),
): FleetTurn[] {
  return prev.map((turn) => turn.id === turnId ? closeReasoningSpan({
    ...turn,
    reply: clearDraft ? "" : turn.reply,
    reasoning: clearDraft ? "" : turn.reasoning,
    thinking: false,
    replyRecovering: true,
    contextProgress: undefined,
  }, nowMs) : turn);
}

/** A turn whose saved row is gone settles with no answer and says why. The
 * unconfirmed draft is dropped, and a row still running ends as failed, so its
 * clock and its wait both stop. */
export function applyReplyGone(prev: FleetTurn[], turnId: string, nowMs: number = Date.now()): FleetTurn[] {
  return prev.map((turn) => turn.id === turnId ? closeReasoningSpan({
    ...turn,
    status: turn.status === AGENTSFLEET_TURN_STATUS.RECEIVED ? AGENTSFLEET_TURN_STATUS.AGENT_ERROR : turn.status,
    reply: "",
    thinking: false,
    replyRecovering: false,
    contextProgress: undefined,
    outcome: OUTCOME.REPLY_GONE,
  }, nowMs) : turn);
}

/** A completed tool-result pass gets its authoritative answer from the row. */
export function applyFinalReply(prev: FleetTurn[], detail: TurnDetail, nowMs: number = Date.now()): FleetTurn[] {
  const index = prev.findIndex((turn) => turn.id === detail.turn_id);
  const turn = prev[index];
  if (turn === undefined) return [...prev, rowToTurn(detail)];
  const activity = readTaskActivityHistory(detail.task_activity);
  const next = applyFinalReplyText(prev, detail.turn_id, detail.response_text ?? "", nowMs);
  return activity ? next.map(row => row.id === detail.turn_id ? {...row, taskActivity: mergeTaskActivity(row.taskActivity, activity)} : row) : next;
}

/** The completion frame carries the same saved answer as the detail route. */
export function applyFinalReplyText(
  prev: FleetTurn[],
  turnId: string,
  reply: string,
  nowMs: number = Date.now(),
): FleetTurn[] {
  const index = prev.findIndex((turn) => turn.id === turnId);
  const turn = prev[index];
  if (turn === undefined) return prev;
  const next = [...prev];
  next[index] = closeReasoningSpan({ ...turn, reply: reply.trim(), thinking: false, replyRecovering: false }, nowMs);
  return next;
}

// The span opens on the first reasoning text and closes on the next answer
// text. A short prompt's model reasons, answers, then reasons again: that
// reopens the span, so the folded Thought counts every stretch, not the
// first. The decoder emits one non-empty kind per delta (`#emit`), so a delta
// without reasoning is answer text, and each end reads its own clock.
function stampReasoningSpan(turn: FleetTurn, delta: ReplyDelta, nowMs: number): FleetTurn {
  if (delta.reasoning.length === 0) return closeReasoningSpan(turn, nowMs);
  return turn.reasoningStartedAtMs === undefined ? { ...turn, reasoningStartedAtMs: nowMs } : reopenReasoningSpan(turn);
}

function reopenReasoningSpan(turn: FleetTurn): FleetTurn {
  const { reasoningEndedAtMs, ...open } = turn;
  return reasoningEndedAtMs === undefined ? turn : open;
}
