import type { ReactNode } from "react";
import { MessagePrimitive, groupPartByType, type MessageState } from "@assistant-ui/react";
import { FleetReplyMarkdown } from "./FleetMarkdown";
import { Waiting } from "./FleetReplyFigures";
import { FleetThought } from "./FleetThought";
import { FleetToolGroup } from "./FleetToolGroup";
import { ToolCallRow } from "./FleetToolCallRow";
import type { readReasoningSpan } from "./fleetMessageReaders";
import { STATUS_RUNNING } from "./fleetReplyMessage";

// Reasoning has one disclosure; each tool call retains its own input, result and clock.
const GROUP = {
  REASONING: "group-reasoning",
  TOOL: "group-tool",
} as const;
const REPLY_GROUP_BY = groupPartByType({
  reasoning: [GROUP.REASONING],
  "tool-call": [GROUP.TOOL],
});

export function FleetReplyParts(reply: ReplyContext) {
  return <MessagePrimitive.GroupedParts groupBy={REPLY_GROUP_BY}>{info => renderReplyPart(info, reply)}</MessagePrimitive.GroupedParts>;
}

export type ReplyContext = {
  errored: boolean;
  running: boolean;
  queued: boolean;
  eventId: string;
  reasoning: string;
  span: ReturnType<typeof readReasoningSpan>;
  /** The message's parts, which a group reads by index. */
  content: MessageState["content"];
  /** When the turn began, for the waiting clock. */
  startedAtMs: number;
  contextProgress?: unknown;
};

/** One switch over every node the library hands back: groups, leaves, the indicator. */
export function renderReplyPart(
  { part, children }: MessagePrimitive.GroupedParts.RenderInfo<(typeof GROUP)[keyof typeof GROUP]>,
  reply: ReplyContext,
): ReactNode {
  switch (part.type) {
    case GROUP.REASONING:
      return (
        <FleetThought
          live={part.status.type === STATUS_RUNNING}
          reasoning={reply.reasoning}
          startedAtMs={reply.span.startedAtMs}
          endedAtMs={reply.span.endedAtMs}
        >
          {children}
        </FleetThought>
      );
    case GROUP.TOOL:
      return <FleetToolGroup content={reply.content} indices={part.indices} running={reply.running} eventId={reply.eventId}>{children}</FleetToolGroup>;
    case "reasoning":
      return <p className="whitespace-pre-wrap text-body-sm leading-prose text-text-dim">{part.text}</p>;
    case "tool-call":
      return (
        <ToolCallRow
          name={part.toolName}
          args={part.args}
          argsText={part.argsText}
          result={part.result}
          running={part.status.type === STATUS_RUNNING}
          settled={!reply.running}
          eventId={reply.eventId}
        />
      );
    case "text":
      return <ReplyText streaming={reply.running} />;
    case "indicator":
      return reply.contextProgress || reply.content.some(item => item.type === "tool-call" && item.result === undefined)
        ? <></> : <Waiting queued={reply.queued} startedAtMs={reply.startedAtMs} />;
    default:
      // A leaf that returns null gets the library's fallback UI; an empty
      // fragment keeps parts this reply never carries invisible.
      return <></>;
  }
}

/**
 * The library owns smoothing and deferred parsing within the current text part.
 * Completing a run updates its status without replacing the displayed answer.
 */
function ReplyText({ streaming }: { streaming: boolean }) {
  return (
    <>
      <FleetReplyMarkdown />
      {streaming ? (
        <span className="sr-only" aria-label="streaming">Receiving reply</span>
      ) : null}
    </>
  );
}

