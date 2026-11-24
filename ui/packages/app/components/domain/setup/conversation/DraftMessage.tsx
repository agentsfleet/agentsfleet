"use client";

import { memo, useMemo } from "react";
import { MessagePrimitive, type MessageState } from "@assistant-ui/react";
import { CheckCircleIcon } from "@phosphor-icons/react/ssr";
import { MessageSchema, MESSAGE_ROLE } from "@/lib/api/draft-types";
import { draftFailureLabel } from "@/lib/api/draft-failure-copy";
import { readContextProgress } from "@/lib/context-progress";
import { FleetMarkdown, FleetReplyMarkdown } from "../../shared/chat/FleetMarkdown";
import { ChatMessageRow } from "../../shared/chat/ChatMessageRow";
import { ContextProgressStatus } from "../../usage/ContextProgressStatus";
import { Timestamp } from "../../shared/identity/FleetTimestamp";
import { ChatProgressLabel, QUEUED_PROGRESS_LABEL } from "../../shared/chat/ChatProgressLabel";
import { ChatReplyProgress } from "@/components/domain/shared/chat/ChatReplyProgress";

const PART_COMPONENTS = {Text: FleetReplyMarkdown};
const SAVING_LABEL = "Saving";

export const DraftMessage = memo(function DraftMessage({message}: {message: MessageState}) {
  const user = message.role === MESSAGE_ROLE.user;
  const text = message.content.filter(part => part.type === "text").map(part => part.text).join("\n");
  const running = message.status?.type === "running";
  const queued = message.metadata.custom.queued === true;
  const admitting = message.metadata.custom.admitting === true;
  const modelPending = message.metadata.custom.pending === true;
  const saving = message.metadata.custom.saving === true;
  const context = useMemo(() => readContextProgress(message.metadata.custom.contextProgress), [message.metadata.custom.contextProgress]);
  if (message.metadata.custom.activity === true) return <MessagePrimitive.Root>
    <p className="my-md inline-flex items-center gap-xs py-xs text-body font-medium text-foreground"><CheckCircleIcon className="size-4 text-success" aria-hidden="true" />{text}</p>
  </MessagePrimitive.Root>;
  return <MessagePrimitive.Root>
    <ChatMessageRow sender={user ? "You" : "Fleet setup"} operator={user} messageRole={message.role} timestamp={<Timestamp createdAt={message.createdAt} />}>
      {saving ? null : text ? user ? <FleetMarkdown expandable>{text}</FleetMarkdown> : <MessagePrimitive.Parts components={PART_COMPONENTS} /> : null}
      {running && context ? <ContextProgressStatus value={context} /> :
        (saving || queued || running) && (saving || !text) ? <DraftProgress label={admitting ? "Sending" : queued ? QUEUED_PROGRESS_LABEL : saving && !modelPending ? SAVING_LABEL : "Thinking"} startedAtMs={typeof message.metadata.custom.startedAtMs === "number" ? message.metadata.custom.startedAtMs : message.createdAt.getTime()} />
        : null}
      {!user && message.metadata.custom.failure ? <DraftFailure message={message} /> : null}
    </ChatMessageRow>
  </MessagePrimitive.Root>;
});

function DraftProgress({label, startedAtMs}: {label: string; startedAtMs: number}) {
  if (label !== SAVING_LABEL) return <ChatReplyProgress label={label} />;
  return <output aria-label={label} aria-live="off" className="inline-flex items-center gap-sm text-body-sm text-muted-foreground"><ChatProgressLabel label={label} startedAtMs={startedAtMs} /></output>;
}

function DraftFailure({message}: {message: MessageState}) {
  const raw = message.metadata.custom.failure;
  const parsed = useMemo(() => MessageSchema.shape.failure.unwrap().safeParse(raw), [raw]);
  if (!parsed.success) return null;
  return <p className="my-sm font-medium text-destructive">{draftFailureLabel(parsed.data)}</p>;
}
