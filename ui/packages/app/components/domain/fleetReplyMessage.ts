import type { ThreadMessageLike } from "@assistant-ui/react";
import { z } from "zod";

import { isSteerBy } from "@/lib/events/event-summary";
import {
  AGENTSFLEET_EVENT_STATUS,
  type FleetEvent,
  type FleetToolCall,
} from "@/lib/streaming/fleet-stream-row";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";

// A reply row as assistant-ui models it: an assistant message whose status is
// the run's, and whose content is the reasoning, tool-call and text parts the
// library groups, times and renders. Pure — a row and its converted base in,
// one message out.

/** Custom-bag keys for the reasoning span; the library has no field for it. */
export const REASONING_SPAN = {
  STARTED: "reasoningStartedAtMs",
  ENDED: "reasoningEndedAtMs",
} as const;

/** Custom-bag keys for what the turn cost and the calls it did not record. On
 * the reply, never the trigger: the trigger's conversion is compared to detect
 * a changed row, and a turn's figures change while its trigger does not. */
export const REPLY_FIGURE = {
  TOKENS: "replyTokens",
  WALL_MS: "replyWallMs",
  COST_NANOS: "replyCostNanos",
  OMITTED_CALLS: "omittedCallCount",
} as const;

// A done call's `result`: what tells the library the call finished, and what
// its cell shows. Each field narrows on its own, as the frames that fill it do.
const TOOL_RESULT = z.object({
  status: z.enum([TOOL_CALL_STATUS.SUCCEEDED, TOOL_CALL_STATUS.FAILED, TOOL_CALL_STATUS.INTERRUPTED]).optional().catch(undefined),
  outputHead: z.string().optional().catch(undefined),
  outputTail: z.string().optional().catch(undefined),
  outputLineCount: z.number().optional().catch(undefined),
  exitCode: z.number().optional().catch(undefined),
  callId: z.string().optional().catch(undefined),
});

export type ToolResult = z.infer<typeof TOOL_RESULT>;

/** The status type assistant-ui gives a message or part that is still running. */
export const STATUS_RUNNING = "running";
const RUNNING = { type: STATUS_RUNNING } as const;
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
const ERROR_STATUSES: ReadonlySet<string> = new Set([TOOL_CALL_STATUS.FAILED, TOOL_CALL_STATUS.INTERRUPTED]);

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
        [REPLY_FIGURE.TOKENS]: event.tokens,
        [REPLY_FIGURE.WALL_MS]: event.wallMs,
        [REPLY_FIGURE.COST_NANOS]: event.costNanos,
        [REPLY_FIGURE.OMITTED_CALLS]: event.omittedCallCount,
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

/** A tool-call part's result, or undefined while the call runs. */
export function readToolResult(result: unknown): ToolResult | undefined {
  return result === undefined ? undefined : TOOL_RESULT.catch({}).parse(result);
}

type PartFields = { result: ToolResult | undefined; timing: { startedAt: number; completedAt?: number } };

// A call's result and timing, by the call object they describe. The reducer
// keeps a call's object until the call itself changes, so a reply converted
// again for a streamed word hands each cell the same objects and its memo
// holds; a WeakMap lets a replaced call's entry go with it.
const PART_FIELDS = new WeakMap<FleetToolCall, PartFields>();

function toolCallPart(eventId: string, tool: FleetToolCall, index: number): ReplyPart {
  const { result, timing } = partFields(tool);
  return {
    type: "tool-call",
    // The runner's own id when it names the call: a saved trace that replaces
    // the live list keeps each call's identity even where positions differ.
    toolCallId: `${eventId}${TOOL_CALL_ID_INFIX}${tool.callId ?? index}`,
    toolName: tool.name,
    // Absent arguments read as `{}` in the library, which is what a call
    // that named none was made with.
    ...(tool.args === undefined ? {} : { args: tool.args }),
    ...(result === undefined ? {} : { result, isError: tool.status !== undefined && ERROR_STATUSES.has(tool.status) }),
    timing,
  };
}

function partFields(tool: FleetToolCall): PartFields {
  const known = PART_FIELDS.get(tool);
  if (known !== undefined) return known;
  const completedAt = tool.done && tool.ms !== null ? tool.startedAtMs + tool.ms : undefined;
  const fields: PartFields = {
    result: tool.done ? toolResult(tool) : undefined,
    timing: completedAt === undefined ? { startedAt: tool.startedAtMs } : { startedAt: tool.startedAtMs, completedAt },
  };
  PART_FIELDS.set(tool, fields);
  return fields;
}

function toolResult({ status, outputHead, outputTail, outputLineCount, exitCode, callId }: FleetToolCall): ToolResult {
  return { status, outputHead, outputTail, outputLineCount, exitCode, callId };
}
