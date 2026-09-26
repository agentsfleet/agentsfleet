import type { EventDetail } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { applyLiveFrame } from "./fleet-stream-frames";
import { closeReasoningSpan, rowToEvent, type FleetEvent } from "./fleet-stream-row";
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

// The span opens on the first reasoning text and closes on the first answer
// text after it. The decoder emits one kind per delta, so each end reads its
// own clock.
function stampReasoningSpan(event: FleetEvent, delta: ReplyDelta, nowMs: number): FleetEvent {
  const opened = delta.reasoning.length > 0 && event.reasoningStartedAtMs === undefined
    ? { ...event, reasoningStartedAtMs: nowMs }
    : event;
  return delta.answer.length > 0 ? closeReasoningSpan(opened, nowMs) : opened;
}
