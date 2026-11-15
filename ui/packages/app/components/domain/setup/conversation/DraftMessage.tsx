"use client";

import { createContext, memo, useCallback, useContext, useMemo, type ReactNode } from "react";
import { MessagePrimitive, type MessageState } from "@assistant-ui/react";
import { CheckCircleIcon } from "@phosphor-icons/react/ssr";
import type { DraftClient } from "@/lib/api/drafts";
import { MessageSchema, MESSAGE_ROLE } from "@/lib/api/draft-types";
import { draftFailureCopy } from "@/lib/api/draft-failure-copy";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { readContextProgress } from "@/lib/context-progress";
import { FleetMarkdown, FleetReplyMarkdown } from "../../shared/chat/FleetMarkdown";
import { ChatMessageRow } from "../../shared/chat/ChatMessageRow";
import { ChatFailure } from "../../shared/chat/ChatFailure";
import { ContextProgressStatus } from "../../usage/ContextProgressStatus";
import { Timestamp } from "../../shared/identity/FleetTimestamp";
import { ChatProgressLabel } from "../../shared/chat/ChatProgressLabel";

const PART_COMPONENTS = {Text: FleetReplyMarkdown};
const RetryContext = createContext<{disabled: boolean; retry: (text: string) => Promise<void>} | null>(null);

/** Streaming text never changes the retry context or makes settled rows scan history. */
export function DraftRetryProvider({client, disabled, children}: {client: DraftClient; disabled: boolean; children: ReactNode}) {
  const retry = useCallback(async (text: string): Promise<void> => {
    const current = client.getSnapshot();
    const recovery = chatRecovery(current);
    if (current.busy || current.pending || current.inaccessible || recovery.pending) return;
    try {
      await client.send(text);
    } catch { /* DraftClient exposes the failure and retains the request. */ }
  }, [client]);
  const value = useMemo(() => ({disabled, retry}), [disabled, retry]);
  return <RetryContext value={value}>{children}</RetryContext>;
}

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
        (saving || queued || running) && (saving || !text) ? <DraftProgress label={admitting ? "Sending" : queued ? "Queued" : saving && !modelPending ? "Saving" : "Thinking"} startedAtMs={typeof message.metadata.custom.startedAtMs === "number" ? message.metadata.custom.startedAtMs : message.createdAt.getTime()} />
        : null}
      {!user && message.metadata.custom.failure ? <DraftFailure message={message} /> : null}
    </ChatMessageRow>
  </MessagePrimitive.Root>;
});

function DraftProgress({label, startedAtMs}: {label: string; startedAtMs: number}) {
  return <output aria-label={label} aria-live="off" className="inline-flex items-center gap-sm text-body-sm text-muted-foreground"><ChatProgressLabel label={label} startedAtMs={startedAtMs} /></output>;
}

function DraftFailure({message}: {message: MessageState}) {
  const recovery = useContext(RetryContext);
  const raw = message.metadata.custom.failure;
  const parsed = useMemo(() => MessageSchema.shape.failure.unwrap().safeParse(raw), [raw]);
  const request = message.metadata.custom.retryText;
  const retry = useMemo(() => typeof request === "string" && recovery ? () => recovery.retry(request) : undefined, [recovery, request]);
  if (!parsed.success) return null;
  const failure = parsed.data;
  return <ChatFailure {...draftFailureCopy(failure)} details={<p>Reference: {failure.error_code}</p>}
    onRetry={retry} disabled={recovery?.disabled} />;
}
