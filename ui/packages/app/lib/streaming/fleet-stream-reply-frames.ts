import type { EventDetail } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { applyLiveFrame } from "./fleet-stream-frames";
import { rowToEvent, type FleetEvent } from "./fleet-stream-row";
import type { ReplyDelta } from "./reply-stream-decoder";

/** Fold already classified text; model protocol never enters the row. */
export function applyReplyDelta(
  prev: FleetEvent[],
  eventId: string,
  delta: ReplyDelta,
): FleetEvent[] {
  const index = prev.findIndex((event) => event.id === eventId);
  if (index < 0) {
    const opened = applyLiveFrame(prev, { kind: FRAME_KIND.CHUNK, event_id: eventId, text: delta.answer });
    return opened.map((event) => event.id === eventId
      ? { ...event, reasoning: delta.reasoning, thinking: delta.thinking }
      : event);
  }
  return prev.map((event, at) => at === index ? {
    ...event,
    reply: event.reply + delta.answer,
    reasoning: (event.reasoning ?? "") + delta.reasoning,
    thinking: delta.thinking,
  } : event);
}

/** Mark a streamed draft as awaiting its durable ending. */
export function applyReplyRecovery(prev: FleetEvent[], eventId: string, clearDraft: boolean): FleetEvent[] {
  return prev.map((event) => event.id === eventId ? {
    ...event,
    reply: clearDraft ? "" : event.reply,
    reasoning: clearDraft ? "" : event.reasoning,
    thinking: false,
    replyRecovering: true,
  } : event);
}

/** A completed tool-result pass gets its authoritative answer from the row. */
export function applyFinalReply(prev: FleetEvent[], detail: EventDetail): FleetEvent[] {
  const index = prev.findIndex((event) => event.id === detail.event_id);
  const event = prev[index];
  if (event === undefined) return [...prev, rowToEvent(detail)];
  const next = [...prev];
  next[index] = { ...event, reply: detail.response_text?.trim() ?? "", thinking: false, replyRecovering: false };
  return next;
}
