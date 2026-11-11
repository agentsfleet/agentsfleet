import type { ReactNode } from "react";
import type { DefaultReplyContent as DefaultContent, ReplyContext } from "./fleet-reply-content";
import { MessagePrimitive, MessagePartPrimitive, groupPartByType } from "@assistant-ui/react";
import { FleetMarkdown, FleetReplyMarkdown } from "./FleetMarkdown";
import { Waiting } from "./FleetReplyFigures";
import { FleetThought } from "./FleetThought";
import { FleetToolGroup } from "./FleetToolGroup";
import { FleetRegisteredTool } from "./FleetToolRegistry";
import { readDefaultReplyContent } from "./fleet-reply-content";
import { STATUS_RUNNING } from "./fleetReplyMessage";

const GROUP = {REASONING: "group-reasoning", TOOL: "group-tool"} as const;
const REPLY_GROUP_BY = groupPartByType({reasoning: [GROUP.REASONING], "tool-call": [GROUP.TOOL]});
type ReplyRenderInfo = MessagePrimitive.GroupedParts.RenderInfo<(typeof GROUP)[keyof typeof GROUP]>;

export function FleetReplyParts(reply: ReplyContext) {
  return <MessagePrimitive.GroupedParts groupBy={REPLY_GROUP_BY}>{info => renderReplyPart(info, reply)}</MessagePrimitive.GroupedParts>;
}

/** The native pipeline resolves registered tool UI and owns part identity. */
export function renderReplyPart({part, children}: ReplyRenderInfo, reply: ReplyContext): ReactNode {
  switch (part.type) {
    case GROUP.REASONING: return <FleetThought live={part.status.type === STATUS_RUNNING} reasoning={reply.reasoning}
      startedAtMs={reply.span.startedAtMs} endedAtMs={reply.span.endedAtMs}>{children}</FleetThought>;
    case GROUP.TOOL: return <FleetToolGroup content={reply.content} indices={part.indices}
      running={reply.running} eventId={reply.eventId}>{children}</FleetToolGroup>;
    case "reasoning": return <p className="whitespace-pre-wrap text-body-sm leading-prose text-text-dim">{part.text}</p>;
    case "tool-call": return part.toolUI ?? <FleetRegisteredTool {...part} />;
    case "text": return <ReplyText streaming={reply.running} plainText={reply.plainText} />;
    case "indicator": return reply.contextProgress || reply.content.some(item => item.type === "tool-call" && item.result === undefined)
      ? <></> : <Waiting queued={reply.queued} startedAtMs={reply.startedAtMs} />;
    default: return <DefaultReplyContent data={readDefaultReplyContent(part)} />;
  }
}

function DefaultReplyContent({data}: {data: DefaultContent}) {
  return <div className="space-y-sm text-label">
    {data.text ? <FleetMarkdown expandable>{data.text}</FleetMarkdown> : data.href ? null
      : <p className="text-muted-foreground">{data.label} · Preview unavailable.</p>}
    {data.href ? <a href={data.href} target="_blank" rel="noreferrer noopener"
      className="break-words underline decoration-border-strong underline-offset-2 hover:decoration-foreground">{data.label}</a> : null}
  </div>;
}

/** Keep deferred Markdown parsing mounted through streaming and completion. */
function ReplyText({streaming, plainText}: {streaming: boolean; plainText?: boolean}) {
  return <>{plainText ? <p className="whitespace-pre-wrap leading-conversation"><MessagePartPrimitive.Text /></p> : <FleetReplyMarkdown />}
    {streaming ? <span className="sr-only" aria-label="streaming">Receiving reply</span> : null}</>;
}
