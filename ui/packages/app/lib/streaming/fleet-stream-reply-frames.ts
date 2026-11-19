import { mergeTaskActivity, readTaskActivityHistory } from "../task-activity";
import type { TurnDetail } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { applyLiveFrame } from "./fleet-stream-frames";
import { OUTCOME } from "@/lib/events/event-summary";
import { AGENTSFLEET_EVENT_STATUS, closeReasoningSpan, rowToEvent, type FleetEvent } from "./fleet-stream-row";
import type { ReplyDelta } from "./reply-stream-decoder";

/** Fold already classified text; model protocol never enters the row. */
export function applyReplyDelta(
  prev: FleetEvent[],
  turnId: string,
  delta: ReplyDelta,
  nowMs: number = Date.now(),
): FleetEvent[] {
  const index = prev.findIndex((event) => event.id === turnId);
  if (index < 0) {
    const opened = applyLiveFrame(prev, { text_kind: "answer", kind: FRAME_KIND.CHUNK, turn_id: turnId, text: delta.answer }, nowMs);
    return opened.map((event) => event.id === turnId
      ? stampReasoningSpan({ ...event, reasoning: delta.reasoning, thinking: delta.thinking }, delta, nowMs)
      : event);
  }
  return prev.map((event, at) => at === index ? stampReasoningSpan({
    ...event,
    reply: event.reply + delta.answer,
    reasoning: (event.reasoning ?? "") + delta.reasoning,
    thinking: delta.thinking,
    contextProgressHidden: event.contextProgress ? true : undefined,
  }, delta, nowMs) : event);
}

/** Mark a streamed draft as awaiting its durable ending. */
export function applyReplyRecovery(
  prev: FleetEvent[],
  turnId: string,
  clearDraft: boolean,
  nowMs: number = Date.now(),
): FleetEvent[] {
  return prev.map((event) => event.id === turnId ? closeReasoningSpan({
    ...event,
    reply: clearDraft ? "" : event.reply,
    reasoning: clearDraft ? "" : event.reasoning,
    thinking: false,
    replyRecovering: true,
    contextProgress: undefined,
  }, nowMs) : event);
}

/** An event whose saved row is gone settles with no answer and says why. The
 * unconfirmed draft is dropped, and a row still running ends as failed, so its
 * clock and its wait both stop. */
export function applyReplyGone(prev: FleetEvent[], turnId: string, nowMs: number = Date.now()): FleetEvent[] {
  return prev.map((event) => event.id === turnId ? closeReasoningSpan({
    ...event,
    status: event.status === AGENTSFLEET_EVENT_STATUS.RECEIVED ? AGENTSFLEET_EVENT_STATUS.AGENT_ERROR : event.status,
    reply: "",
    thinking: false,
    replyRecovering: false,
    contextProgress: undefined,
    outcome: OUTCOME.REPLY_GONE,
  }, nowMs) : event);
}

/** A completed tool-result pass gets its authoritative answer from the row. */
export function applyFinalReply(prev: FleetEvent[], detail: TurnDetail, nowMs: number = Date.now()): FleetEvent[] {
  const index = prev.findIndex((event) => event.id === detail.turn_id);
  const event = prev[index];
  if (event === undefined) return [...prev, rowToEvent(detail)];
  const activity = readTaskActivityHistory(detail.task_activity);
  const next = applyFinalReplyText(prev, detail.turn_id, detail.response_text ?? "", nowMs);
  return activity ? next.map(row => row.id === detail.turn_id ? {...row, taskActivity: mergeTaskActivity(row.taskActivity, activity)} : row) : next;
}

/** The completion frame carries the same saved answer as the detail route. */
export function applyFinalReplyText(
  prev: FleetEvent[],
  turnId: string,
  reply: string,
  nowMs: number = Date.now(),
): FleetEvent[] {
  const index = prev.findIndex((event) => event.id === turnId);
  const event = prev[index];
  if (event === undefined) return prev;
  const next = [...prev];
  next[index] = closeReasoningSpan({ ...event, reply: reply.trim(), thinking: false, replyRecovering: false }, nowMs);
  return next;
}

// The span opens on the first reasoning text and closes on the next answer
// text. A short prompt's model reasons, answers, then reasons again: that
// reopens the span, so the folded Thought counts every stretch, not the
// first. The decoder emits one non-empty kind per delta (`#emit`), so a delta
// without reasoning is answer text, and each end reads its own clock.
function stampReasoningSpan(event: FleetEvent, delta: ReplyDelta, nowMs: number): FleetEvent {
  if (delta.reasoning.length === 0) return closeReasoningSpan(event, nowMs);
  return event.reasoningStartedAtMs === undefined ? { ...event, reasoningStartedAtMs: nowMs } : reopenReasoningSpan(event);
}

function reopenReasoningSpan(event: FleetEvent): FleetEvent {
  const { reasoningEndedAtMs, ...open } = event;
  return reasoningEndedAtMs === undefined ? event : open;
}
