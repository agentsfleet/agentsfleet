"use client";

import { createContext, memo, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import { MessagePrimitive, type MessageState } from "@assistant-ui/react";
import { Button, SignalSpinner } from "@agentsfleet/design-system";
import type { DraftClient } from "@/lib/api/drafts";
import { MessageSchema, MESSAGE_ROLE } from "@/lib/api/draft-types";
import { draftFailureCopy } from "@/lib/api/draft-failure-copy";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { readContextProgress } from "@/lib/context-progress";
import { FleetMarkdown, FleetReplyMarkdown } from "../FleetMarkdown";
import { ChatMessageRow } from "./ChatMessageRow";
import { ChatFailure } from "./ChatFailure";
import { ContextProgressStatus } from "../ContextProgressStatus";

const PART_COMPONENTS = {Text: FleetReplyMarkdown};
const RetryContext = createContext<{disabled: boolean; retry: (text: string) => Promise<void>} | null>(null);
const LONG_MESSAGE_CHARACTERS = 1000;
const MESSAGE_PREVIEW_CHARACTERS = 600;

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
  const saving = message.metadata.custom.saving === true;
  const context = useMemo(() => readContextProgress(message.metadata.custom.contextProgress), [message.metadata.custom.contextProgress]);
  if (message.metadata.custom.activity === true) return <MessagePrimitive.Root>
    <p className="my-md py-xs text-body font-medium text-foreground">{text}</p>
  </MessagePrimitive.Root>;
  return <MessagePrimitive.Root>
    <ChatMessageRow sender={user ? "You" : "Fleet setup"} operator={user} messageRole={message.role}>
      {saving ? <output aria-label="Saving setup" className="inline-flex items-center gap-sm text-label text-muted-foreground"><SignalSpinner className="text-pulse" />Saving setup...</output>
        : text ? user ? text.length > LONG_MESSAGE_CHARACTERS ? <LongUserMessage text={text} /> : <FleetMarkdown>{text}</FleetMarkdown> : <MessagePrimitive.Parts components={PART_COMPONENTS} /> : null}
      {saving ? null : running && context ? <ContextProgressStatus value={context} /> :
        queued && !text ? <output aria-label="Queued" className="text-label text-muted-foreground">Queued…</output>
        : running && !text ? <output aria-label="Working" className="inline-flex items-center gap-sm text-label text-muted-foreground"><SignalSpinner className="text-pulse" />Working…</output>
        : null}
      {!user && message.metadata.custom.failure ? <DraftFailure message={message} /> : null}
    </ChatMessageRow>
  </MessagePrimitive.Root>;
});

function LongUserMessage({text}: {text: string}) {
  const [expanded, setExpanded] = useState(false);
  return <>
    {expanded ? <FleetMarkdown>{text}</FleetMarkdown> : <p className="whitespace-pre-wrap">{text.slice(0, MESSAGE_PREVIEW_CHARACTERS).trimEnd()}…</p>}
    <Button variant="link" size="sm" className="px-0" aria-expanded={expanded} onClick={() => setExpanded(value => !value)}>
      {expanded ? "Show less" : "Show full message"}
    </Button>
  </>;
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
