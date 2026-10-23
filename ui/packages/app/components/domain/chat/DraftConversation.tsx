"use client";

import type { AppendMessage, MessageState, ThreadMessageLike } from "@assistant-ui/react";
import type { DraftView } from "@/lib/api/draft-state";
import type { DraftClient } from "@/lib/api/drafts";
import type { ReactNode } from "react";
import { useMemo } from "react";
import { AssistantRuntimeProvider, MessagePrimitive, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";
import { GitPullRequestIcon, RadarIcon, SunriseIcon } from "lucide-react";
import { SignalSpinner, BrandMark, Button, PageTitle, cn } from "@agentsfleet/design-system";
import { MESSAGE_ROLE, MESSAGE_STATE } from "@/lib/api/draft-types";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { FleetReplyMarkdown } from "../FleetMarkdown";
import { CHAT_EXAMPLES } from "./chat-defaults";
import { ChatComposer } from "./ChatComposer";
import { ChatThread } from "./ChatThread";
import { ChatMessageRow } from "./ChatMessageRow";
import { DraftSaveBar } from "./DraftSaveBar";
import { draftThreadMessages } from "@/lib/api/draft-thread-messages";

export function DraftConversation({client, view, children}: {client: DraftClient; view: DraftView; children?: ReactNode}) {
  const recovery = chatRecovery(view);
  const messages = useMemo(() => draftThreadMessages(view), [view]);
  const anchor = setupAnchor(view);
  const adapter = useMemo(() => ({
    messages, isRunning: recovery.pending || Boolean(view.optimistic), convertMessage: (message: ThreadMessageLike) => message,
    onNew: async (message: AppendMessage) => {
      const text = message.content.filter((part) => part.type === "text").map((part) => part.text).join("\n");
      await client.send(text);
    },
    onCancel: () => client.stop(),
  }), [client, messages, recovery.pending, view.optimistic]);
  const runtime = useExternalStoreRuntime(adapter);
  const empty = messages.length === 0 && !view.fleet && !view.draft && !view.preview;
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <ChatThread label="Private fleet setup conversation" centered={empty}
        footer={<><ChatComposer client={client} view={view} canStop={recovery.canStop} blocked={recovery.blocked} /><DraftSaveBar client={client} view={view} /></>}
        after={empty ? <ChatSuggestions disabled={view.busy || view.pending || view.inaccessible} /> : undefined}>
        {empty ? <ChatEntrance /> : null}
        <div role="log" aria-live="off" className={cn("mx-auto flex w-full max-w-measure flex-col", !empty && "flex-1 py-lg")}>
          {view.truncated ? <p className="text-sm text-muted-foreground">Showing the latest conversation messages. Earlier messages remain in this private draft.</p> : null}
          {anchor === null ? children : null}
          <ThreadPrimitive.Messages>{({message}) => <><DraftMessage message={message} />{message.id === anchor ? children : null}</>}</ThreadPrimitive.Messages>
        </div>
      </ChatThread>
    </AssistantRuntimeProvider>
  );
}
function setupAnchor(view: DraftView): string | null {
  const assistants = view.messages.filter((message) => message.role === MESSAGE_ROLE.assistant);
  const origin = assistants.find((message) => message.candidate_id === view.draft?.candidate?.id && message.candidate_id !== undefined)
    ?? assistants.findLast((message) => message.state === MESSAGE_STATE.completed);
  return origin ? `${origin.sequence}:${origin.role}` : null;
}

function ChatEntrance() {
  return <div className="content-entrance mx-auto flex w-full max-w-measure flex-col items-center gap-lg pb-xl pt-2xl text-center">
    <BrandMark className="size-10 text-pulse" aria-hidden="true" />
    <PageTitle>What should your fleet do?</PageTitle>
    <p className="text-body text-muted-foreground">Start with a job. We’ll work out the setup together.</p>
  </div>;
}

const SUGGESTIONS = [
  { title: "Fix a build", detail: "Find what broke and why", Icon: GitPullRequestIcon, prompt: CHAT_EXAMPLES[0] },
  { title: "Watch for errors", detail: "Catch issues and plan a repair", Icon: RadarIcon, prompt: CHAT_EXAMPLES[1] },
  { title: "Morning brief", detail: "Know what needs your attention", Icon: SunriseIcon, prompt: CHAT_EXAMPLES[2] },
] as const;

function ChatSuggestions({ disabled }: { disabled: boolean }) {
  return <div className="mx-auto grid w-full max-w-measure grid-cols-1 gap-md pb-2xl pt-md sm:grid-cols-3">
    {SUGGESTIONS.map(({ title, detail, Icon, prompt }) => <ThreadPrimitive.Suggestion key={title} prompt={prompt} send={false} asChild>
      <Button variant="outline" size="card" disabled={disabled} className="flex-row items-center justify-start gap-lg whitespace-normal text-left sm:min-h-28 sm:flex-col sm:items-start">
        <Icon size={18} className="text-pulse" aria-hidden="true" />
        <span className="flex flex-col gap-sm"><span>{title}</span><span className="text-label font-normal text-muted-foreground">{detail}</span></span>
      </Button>
    </ThreadPrimitive.Suggestion>)}
  </div>;
}

function DraftMessage({message}: {message: MessageState}) {
  const user = message.role === MESSAGE_ROLE.user;
  const text = message.content.filter((part) => part.type === "text").map((part) => part.text).join("\n");
  const running = message.status?.type === "running";
  const queued = message.metadata.custom.queued === true;
  return <MessagePrimitive.Root className="py-md">
    <ChatMessageRow sender={user ? "You" : "Fleet setup"} operator={user} messageRole={message.role}>
      {queued && !text ? <output aria-label="Queued" className="text-label text-muted-foreground">Queued…</output>
        : running && !text ? <output aria-label="Working" className="inline-flex items-center gap-sm text-label text-muted-foreground"><SignalSpinner className="text-pulse" />Working…</output>
        : <FleetReplyMarkdown text={text} streaming={running} />}
    </ChatMessageRow>
  </MessagePrimitive.Root>;
}
