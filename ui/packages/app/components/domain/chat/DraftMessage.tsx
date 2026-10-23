"use client";

import { createContext, memo, useCallback, useContext, useMemo, type ReactNode } from "react";
import { MessagePrimitive, type MessageState } from "@assistant-ui/react";
import { SignalSpinner } from "@agentsfleet/design-system";
import type { DraftClient } from "@/lib/api/drafts";
import { MessageSchema, MESSAGE_ROLE } from "@/lib/api/draft-types";
import { draftFailureCopy } from "@/lib/api/draft-failure-copy";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { FleetReplyMarkdown } from "../FleetMarkdown";
import { ChatMessageRow } from "./ChatMessageRow";
import { ChatFailure } from "./ChatFailure";

const RetryContext = createContext<{disabled: boolean; retry: (text: string) => Promise<void>} | null>(null);

/** Streaming text never changes the retry context or makes settled rows scan history. */
export function DraftRetryProvider({client, disabled, children}: {client: DraftClient; disabled: boolean; children: ReactNode}) {
  const retry = useCallback(async (text: string): Promise<void> => {
    const current = client.getSnapshot();
    const recovery = chatRecovery(current);
    if (current.busy || current.pending || current.inaccessible || recovery.pending) return;
    try {
      if (recovery.blocked) await client.stop();
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
  return <MessagePrimitive.Root>
    <ChatMessageRow sender={user ? "You" : "Fleet setup"} operator={user} messageRole={message.role}>
      {queued && !text ? <output aria-label="Queued" className="text-label text-muted-foreground">Queued…</output>
        : running && !text ? <output aria-label="Working" className="inline-flex items-center gap-sm text-label text-muted-foreground"><SignalSpinner className="text-pulse" />Working…</output>
        : text ? <FleetReplyMarkdown text={text} streaming={running} /> : null}
      {!user && message.metadata.custom.failure ? <DraftFailure message={message} /> : null}
    </ChatMessageRow>
  </MessagePrimitive.Root>;
});

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
