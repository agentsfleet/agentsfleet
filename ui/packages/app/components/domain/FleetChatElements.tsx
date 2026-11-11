"use client";

import { useState, type ReactNode } from "react";
import { senderLabelFor } from "@/lib/events/event-summary";
import { FleetMessageRow, FleetGroupRow, ROW_TONE, useSenderLabel } from "./FleetMessageRow";
import { FleetMarkdown } from "./FleetMarkdown";
import { FleetReply } from "./FleetReplyBody";
import { MessageUploadAttachments } from "./chat/UploadAttachments";
import { useNameSender } from "./SenderNames";
import { IntegrationActivityElement, WakeupActivityElement, ApprovalActivityElement, DefaultActivityElement } from "./FleetActivityElements";
import { CHAT_ELEMENT, type ChatElementKind, type FleetChatElement, type FleetChatElementData } from "./fleet-chat-elements";

type ElementRenderers = {[K in ChatElementKind]: (data: FleetChatElementData[K]) => ReactNode};

/** Trusted static registration. Every kind must register its exact payload;
 * backend content cannot register a component or executable configuration. */
const ELEMENT_RENDERERS: Readonly<ElementRenderers> = Object.freeze({
  [CHAT_ELEMENT.OPERATOR]: data => <OperatorElement data={data} />,
  [CHAT_ELEMENT.SUBMISSION]: data => <UploadSubmissionElement data={data} />,
  [CHAT_ELEMENT.REPLY]: data => <ReplyElement data={data} />,
  [CHAT_ELEMENT.INTEGRATION]: data => <IntegrationActivityElement data={data} />,
  [CHAT_ELEMENT.WAKEUP]: data => <WakeupActivityElement data={data} />,
  [CHAT_ELEMENT.APPROVAL]: data => <ApprovalActivityElement data={data} />,
  [CHAT_ELEMENT.GROUP]: data => <ActivityGroupElement data={data} />,
  [CHAT_ELEMENT.DEFAULT]: data => <DefaultActivityElement data={data} />,
});

export function FleetChatElementView<K extends ChatElementKind>({element}: {element: FleetChatElement<K>}) {
  return ELEMENT_RENDERERS[element.kind](element.data);
}

function ReplyElement({data}: {data: FleetChatElementData["reply-turn"]}) {
  return <FleetReply {...data} senderLabel={useSenderLabel()} />;
}

function OperatorElement({data}: {data: FleetChatElementData["operator-turn"]}) {
  const sender = useNameSender()(data.actor);
  return <FleetMessageRow sender={sender.label} tone={ROW_TONE.OPERATOR} messageRole="user"
    dimmed={data.optimistic} showSender={sender.shown} createdAt={data.createdAt}>
    {data.text.trim() ? <FleetMarkdown expandable>{data.text}</FleetMarkdown> : <p className="text-muted-foreground">Content preview unavailable.</p>}
    <MessageUploadAttachments />
  </FleetMessageRow>;
}

function UploadSubmissionElement({data}: {data: FleetChatElementData["upload-submission"]}) {
  return <FleetMessageRow sender="You" tone={ROW_TONE.OPERATOR} messageRole="user" dimmed createdAt={data.createdAt}>
    {data.text.trim() ? <FleetMarkdown expandable>{data.text}</FleetMarkdown> : <p>File preview</p>}
    <MessageUploadAttachments preparing />
  </FleetMessageRow>;
}

function ActivityGroupElement({data}: {data: FleetChatElementData["activity-group"]}) {
  const [expanded, setExpanded] = useState(false);
  const senderLabel = useSenderLabel();
  const newest = data.members.reduce((_, member) => member).data;
  const last = new Date(Math.max(...data.members.map(member => member.data.createdAt.getTime())));
  return <FleetGroupRow sender={senderLabelFor(newest.actor, senderLabel)} headline={newest.headline}
    outcome={newest.outcome} failed={newest.failed} count={data.members.length} last={last}
    expanded={expanded} onToggle={() => setExpanded(value => !value)}>
    {expanded ? data.members.map(member => <FleetChatElementView key={member.data.eventId} element={member} />) : null}
  </FleetGroupRow>;
}

