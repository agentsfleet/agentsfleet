import type { ThreadMessageLike } from "@assistant-ui/react";

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
  AGENTSFLEET_EVENT_STATUS.RECEIVED,
]);
const TOOL_CALL_ID_INFIX = ":tool:";
const NO_ARGS = {} as const;
// The wire carries no tool output. A defined result is what tells the library
// the call finished, so a done call carries this.
const NO_OUTPUT = null;

type ReplyPart = Exclude<ThreadMessageLike["content"], string>[number];

/**
 * Re-shape a converted row as its reply. `base` carries the row's custom bag
 * (status, outcome, failure, timing), so the reply reads the same fields the
 * trigger does.
 */
export function toReplyMessage(base: ThreadMessageLike, event: FleetEvent): ThreadMessageLike {
  return {
    ...base,
    role: "assistant",
    // Per message, never the thread's `isRunning`: that would disable the
    // composer, and a working fleet is no reason to stop an operator steering.
    status: IN_FLIGHT.has(event.status) ? RUNNING : MESSAGE_COMPLETE,
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
