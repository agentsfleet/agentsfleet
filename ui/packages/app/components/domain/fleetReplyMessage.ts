import type { ThreadMessageLike } from "@assistant-ui/react";

import { isSteerBy } from "@/lib/events/event-summary";
import {
  AGENTSFLEET_EVENT_STATUS,
  type FleetEvent,
  type FleetToolCall,
} from "@/lib/streaming/fleet-stream-row";

// A reply row as assistant-ui models it: an assistant message whose status is
// the run's, and whose content is the reasoning, tool-call and text parts the
// library groups, times and renders. Pure — a row and its converted base in,
// one message out.

/** Custom-bag keys for the reasoning span; the library has no field for it. */
export const REASONING_SPAN = {
  STARTED: "reasoningStartedAtMs",
  ENDED: "reasoningEndedAtMs",
} as const;

const RUNNING = { type: "running" } as const;
// A finished turn stopped normally; the message status needs the reason, a
// part's status does not.
const MESSAGE_COMPLETE = { type: "complete", reason: "stop" } as const;
const PART_COMPLETE = { type: "complete" } as const;
const IN_FLIGHT: ReadonlySet<string> = new Set([
  AGENTSFLEET_EVENT_STATUS.OPTIMISTIC,
  AGENTSFLEET_EVENT_STATUS.QUEUED,
  AGENTSFLEET_EVENT_STATUS.RECEIVED,
]);
const TOOL_CALL_ID_INFIX = ":tool:";
const NO_ARGS = {} as const;
// The wire carries no tool output. A defined result is what tells the library
// the call finished, so a done call carries this.
const NO_OUTPUT = null;

type ReplyPart = Exclude<ThreadMessageLike["content"], string>[number];

/** Whether this event's reply is still running: the rule behind a reply
 * row's status. */
export function isReplyInFlight(event: FleetEvent): boolean {
  return IN_FLIGHT.has(event.status);
}

/** Whether the thread reports a run: the newest turn is one this tab sent,
 * and its reply is still running. The viewport's top anchor pins a running
 * turn to the top wherever the reader is, so a turn a teammate, the API, a
 * webhook, or the same operator in another tab sent never engages it, and a
 * reader back in the history stays where they are. */
export function reportsOwnRun(events: readonly FleetEvent[], subject: string | null): boolean {
  const newest = events.at(-1);
  if (newest === undefined || !isReplyInFlight(newest) || !isSteerBy(newest.actor, subject)) return false;
  // A row this tab painted keeps its submit clock through every frame, and
  // another tab's row never had one. A turn announced before its 202 waits in
  // the stream (`HeldTurns`) until the 202 says whose it is.
  return newest.submittedAtMs !== undefined;
}

/**
 * Re-shape a converted row as its reply. `base` carries the row's custom bag
 * (status, outcome, failure, timing), so the reply reads the same fields the
 * trigger does.
 */
export function toReplyMessage(base: ThreadMessageLike, event: FleetEvent): ThreadMessageLike {
  return {
    ...base,
    role: "assistant",
    status: isReplyInFlight(event) ? RUNNING : MESSAGE_COMPLETE,
    content: replyParts(event),
    metadata: {
      ...base.metadata,
      custom: {
        ...base.metadata?.custom,
        [REASONING_SPAN.STARTED]: event.reasoningStartedAtMs,
        [REASONING_SPAN.ENDED]: event.reasoningEndedAtMs,
      },
    },
  };
}

/** Reasoning, then each tool call in first-seen order, then the answer. */
export function replyParts(event: FleetEvent): ReplyPart[] {
  const parts: ReplyPart[] = [];
  const reasoning = event.reasoning ?? "";
  if (reasoning.length > 0) {
    // The part's own status: the thought stops running when the answer
    // starts, while the message keeps running until the turn ends.
    parts.push({ type: "reasoning", text: reasoning, status: event.thinking === true ? RUNNING : PART_COMPLETE });
  }
  (event.tools ?? []).forEach((tool, index) => parts.push(toolCallPart(event.id, tool, index)));
  const answer = event.reply.trim();
  if (answer.length > 0) parts.push({ type: "text", text: answer });
  return parts;
}

function toolCallPart(eventId: string, tool: FleetToolCall, index: number): ReplyPart {
  const completedAt = tool.done && tool.ms !== null ? tool.startedAtMs + tool.ms : undefined;
  return {
    type: "tool-call",
    // Append-only per event, so the index is a stable identity.
    toolCallId: `${eventId}${TOOL_CALL_ID_INFIX}${index}`,
    toolName: tool.name,
    args: NO_ARGS,
    ...(tool.done ? { result: NO_OUTPUT } : {}),
    timing: completedAt === undefined ? { startedAt: tool.startedAtMs } : { startedAt: tool.startedAtMs, completedAt },
  };
}
