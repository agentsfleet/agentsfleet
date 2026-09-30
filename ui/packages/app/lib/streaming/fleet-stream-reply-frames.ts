import type { EventDetail } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { applyLiveFrame } from "./fleet-stream-frames";
import { OUTCOME } from "@/lib/events/event-summary";
import { AGENTSFLEET_EVENT_STATUS, closeReasoningSpan, rowToEvent, type FleetEvent } from "./fleet-stream-row";
import type { ReplyDelta } from "./reply-stream-decoder";

/** Fold already classified text; model protocol never enters the row. */
export function applyReplyDelta(
  prev: FleetEvent[],
  eventId: string,
  delta: ReplyDelta,
  nowMs: number = Date.now(),
): FleetEvent[] {
  const index = prev.findIndex((event) => event.id === eventId);
  if (index < 0) {
    const opened = applyLiveFrame(prev, { kind: FRAME_KIND.CHUNK, event_id: eventId, text: delta.answer }, nowMs);
    return opened.map((event) => event.id === eventId
      ? stampReasoningSpan({ ...event, reasoning: delta.reasoning, thinking: delta.thinking }, delta, nowMs)
      : event);
  }
  return prev.map((event, at) => at === index ? stampReasoningSpan({
    ...event,
    reply: event.reply + delta.answer,
    reasoning: (event.reasoning ?? "") + delta.reasoning,
    thinking: delta.thinking,
  }, delta, nowMs) : event);
}

/** Mark a streamed draft as awaiting its durable ending. */
export function applyReplyRecovery(
  prev: FleetEvent[],
  eventId: string,
  clearDraft: boolean,
  nowMs: number = Date.now(),
): FleetEvent[] {
  return prev.map((event) => event.id === eventId ? closeReasoningSpan({
    ...event,
    reply: clearDraft ? "" : event.reply,
    reasoning: clearDraft ? "" : event.reasoning,
    thinking: false,
    replyRecovering: true,
  }, nowMs) : event);
}

/** An event whose saved row is gone settles with no answer and says why. The
 * unconfirmed draft is dropped, and a row still running ends as failed, so its
 * clock and its wait both stop. */
export function applyReplyGone(prev: FleetEvent[], eventId: string, nowMs: number = Date.now()): FleetEvent[] {
  return prev.map((event) => event.id === eventId ? closeReasoningSpan({
    ...event,
    status: event.status === AGENTSFLEET_EVENT_STATUS.RECEIVED ? AGENTSFLEET_EVENT_STATUS.AGENT_ERROR : event.status,
    reply: "",
    thinking: false,
    replyRecovering: false,
    outcome: OUTCOME.REPLY_GONE,
  }, nowMs) : event);
}

/** A completed tool-result pass gets its authoritative answer from the row. */
export function applyFinalReply(prev: FleetEvent[], detail: EventDetail, nowMs: number = Date.now()): FleetEvent[] {
  const index = prev.findIndex((event) => event.id === detail.event_id);
  const event = prev[index];
  if (event === undefined) return [...prev, rowToEvent(detail)];
  return applyFinalReplyText(prev, detail.event_id, detail.response_text ?? "", nowMs);
}

/** The completion frame carries the same saved answer as the detail route. */
export function applyFinalReplyText(
  prev: FleetEvent[],
  eventId: string,
  reply: string,
  nowMs: number = Date.now(),
): FleetEvent[] {
  const index = prev.findIndex((event) => event.id === eventId);
  const event = prev[index];
  if (event === undefined) return prev;
  const next = [...prev];
  next[index] = closeReasoningSpan({ ...event, reply: reply.trim(), thinking: false, replyRecovering: false }, nowMs);
  return next;
}

// The span opens on the first reasoning text and closes on the next answer
// text. A short prompt's model reasons, answers, then reasons again: that
// reopens the span, so the folded Thought counts every stretch, not the
// first. The decoder emits one kind per delta, so each end reads its own clock.
function stampReasoningSpan(event: FleetEvent, delta: ReplyDelta, nowMs: number): FleetEvent {
  if (delta.reasoning.length > 0) {
    return event.reasoningStartedAtMs === undefined ? { ...event, reasoningStartedAtMs: nowMs } : reopenReasoningSpan(event);
  }
  return delta.answer.length > 0 ? closeReasoningSpan(event, nowMs) : event;
}

function reopenReasoningSpan(event: FleetEvent): FleetEvent {
  const { reasoningEndedAtMs, ...open } = event;
  return reasoningEndedAtMs === undefined ? event : open;
}
