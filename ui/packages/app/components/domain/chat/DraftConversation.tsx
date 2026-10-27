"use client";

import type { AppendMessage, ThreadMessageLike } from "@assistant-ui/react";
import type { DraftView } from "@/lib/api/draft-state";
import type { DraftClient } from "@/lib/api/drafts";
import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import { AssistantRuntimeProvider, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";
import { BrandMark, Button, PageTitle } from "@agentsfleet/design-system";
import { MESSAGE_ROLE, MESSAGE_STATE } from "@/lib/api/draft-types";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { CHAT_EXAMPLES } from "./chat-defaults";
import { ChatComposer } from "./ChatComposer";
import { ChatThread } from "./ChatThread";
import { createDraftThreadProjection } from "@/lib/api/draft-thread-messages";
import { ACTIVITY_KIND, readFleetActivity } from "@/lib/events/fleet-activity";
import { DraftMessage, DraftRetryProvider } from "./DraftMessage";

export function DraftConversation({client, view, children, unfinished, instructionsDirty = false}: {client: DraftClient; view: DraftView; children?: ReactNode; unfinished?: ReactNode; instructionsDirty?: boolean}) {
  const recovery = chatRecovery(view);
  const [project] = useState(createDraftThreadProjection);
  const messages = useMemo(() => project(view), [project, view]);
  const anchor = setupAnchor(view);
  const saving = Boolean(view.saveOperation) && !view.error;
  const adapter = useMemo(() => ({
    messages, isRunning: recovery.pending || Boolean(view.optimistic) || saving, convertMessage: (message: ThreadMessageLike) => message,
    onNew: async (message: AppendMessage) => {
      const text = message.content.filter((part) => part.type === "text").map((part) => part.text).join("\n");
      await client.send(text);
    },
    onCancel: () => client.stop(),
  }), [client, messages, recovery.pending, view.optimistic, saving]);
  const runtime = useExternalStoreRuntime(adapter);
  const empty = messages.length === 0 && !view.fleet && !view.draft && !view.preview;
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <DraftRetryProvider client={client} disabled={view.busy || view.pending || recovery.pending || view.inaccessible || !view.model}>
      <ChatThread label="Fleet setup conversation" autoScroll={!empty}
        footer={<ChatComposer client={client} view={view} canStop={recovery.canStop} blocked={recovery.blocked || instructionsDirty} />}>
        <div role="log" aria-live="off" className="mx-auto flex w-full max-w-chat flex-1 flex-col py-3xl">
          {empty ? <><ChatEntrance /><ChatSuggestions disabled={view.busy || view.pending || view.inaccessible} />{unfinished}</> : null}
          {view.truncated ? <p className="text-sm text-muted-foreground">Showing the latest conversation messages. Earlier messages remain in this setup chat.</p> : null}
          {anchor === null ? children : null}
          <ThreadPrimitive.Messages>{({message}) => <><DraftMessage message={message} />{message.id === anchor ? children : null}</>}</ThreadPrimitive.Messages>
        </div>
      </ChatThread>
      </DraftRetryProvider>
    </AssistantRuntimeProvider>
  );
}
function setupAnchor(view: DraftView): string | null {
  const saved = (view.activities ?? []).find(row => {
    const activity = readFleetActivity(row.request_json);
    return activity && (activity.kind === ACTIVITY_KIND.setupSaved || activity.kind === ACTIVITY_KIND.setupReady)
      && activity.revision === view.fleet?.setup_revision;
  });
  if (saved) return `activity:${saved.event_id}`;
  const previous = view.previousDrafts?.at(-1);
  if (view.fleet && previous) {
    const origin = previous.messages.findLast(message => message.role === MESSAGE_ROLE.assistant);
    return origin ? `${previous.id}:${origin.sequence}:${origin.role}` : null;
  }
  const assistants = view.messages.filter((message) => message.role === MESSAGE_ROLE.assistant);
  const origin = assistants.find((message) => message.candidate_id === view.draft?.candidate?.id && message.candidate_id !== undefined)
    ?? assistants.findLast((message) => message.state === MESSAGE_STATE.completed);
  return origin ? `${origin.sequence}:${origin.role}` : null;
}

function ChatEntrance() {
  return <div className="content-entrance flex w-full flex-col items-start gap-md pb-md pt-5">
    <BrandMark className="mb-sm size-8 text-pulse" aria-hidden="true" />
    <PageTitle>What should your fleet do?</PageTitle>
    <p className="text-body text-muted-foreground">Start with a job. We’ll work out the setup together.</p>
  </div>;
}

const SUGGESTIONS = [
  { title: "Review pull requests", prompt: CHAT_EXAMPLES[0] },
  { title: "Watch for errors", prompt: CHAT_EXAMPLES[1] },
  { title: "Morning brief", prompt: CHAT_EXAMPLES[2] },
] as const;

function ChatSuggestions({ disabled }: { disabled: boolean }) {
  return <div className="flex w-full flex-wrap gap-md pb-3xl pt-lg">
    {SUGGESTIONS.map(({ title, prompt }) => <ThreadPrimitive.Suggestion key={title} prompt={prompt} send={false} asChild>
      <Button variant="outline" size="sm" disabled={disabled} className="text-eyebrow font-normal">{title}</Button>
    </ThreadPrimitive.Suggestion>)}
  </div>;
}
