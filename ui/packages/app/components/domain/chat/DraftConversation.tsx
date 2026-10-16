"use client";

import type { AppendMessage, MessageState, ThreadMessageLike } from "@assistant-ui/react";
import type { DraftView } from "@/lib/api/draft-state";
import type { Message } from "@/lib/api/draft-types";
import type { DraftClient } from "@/lib/api/drafts";
import { useMemo } from "react";
import { AssistantRuntimeProvider, MessagePrimitive, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";
import { GitPullRequestIcon, RadarIcon, SunriseIcon } from "lucide-react";
import { SignalSpinner, BrandMark, Button, PageTitle, cn } from "@agentsfleet/design-system";
import { isRunning, MESSAGE_ROLE, MESSAGE_STATE } from "@/lib/api/draft-types";
import { FleetReplyMarkdown } from "../FleetMarkdown";
import { CHAT_EXAMPLES } from "./chat-defaults";
import { ChatComposer } from "./ChatComposer";

const FAILURE_COPY: Readonly<Record<string, string>> = {
  owner_billing: "Ask the account owner to add funds, then send a new message.",
  choose_authoring_model: "Choose an available authoring model, then send a new message.",
  reduce_context: "Shorten the request or instructions before trying again.",
  retry_message: "Send a new message to try again.",
  await_accounting: "The provider outcome is still being reconciled. Wait before retrying.",
  review_setup: "Review setup again before continuing.",
};

export function DraftConversation({client, view, onReview}: {client: DraftClient; view: DraftView; onReview: () => void}) {
  const running = view.messages.some(isRunning);
  const adapter = useMemo(() => ({
    messages: view.messages, isRunning: running, convertMessage: toThreadMessage,
    onNew: async (message: AppendMessage) => {
      const text = message.content.filter((part) => part.type === "text").map((part) => part.text).join("\n");
      await client.send(text);
    },
    onCancel: () => client.stop(),
  }), [client, view.messages, running]);
  const runtime = useExternalStoreRuntime<Message>(adapter);
  const empty = view.messages.length === 0;
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <ThreadPrimitive.Root className="flex min-h-0 flex-1 flex-col" aria-label="Private fleet setup conversation">
        <ThreadPrimitive.Viewport turnAnchor="top" autoScroll className={cn("flex min-h-0 flex-1 flex-col overflow-y-auto px-sm sm:px-lg", empty && "justify-center")}>
          {empty ? <ChatEntrance /> : null}
          <div role="log" aria-live="off" className={cn("mx-auto flex w-full max-w-measure flex-col", !empty && "flex-1 py-lg")}>
            {view.truncated ? <p className="text-sm text-muted-foreground">Showing the latest conversation messages. Earlier messages remain in this private draft.</p> : null}
            <ThreadPrimitive.Messages>{renderMessage}</ThreadPrimitive.Messages>
          </div>
          <ThreadPrimitive.ViewportFooter className={cn("mx-auto w-full max-w-measure bg-background py-md", !empty && "sticky bottom-0")}>
            <ChatComposer client={client} view={view} running={running} onReview={onReview} />
          </ThreadPrimitive.ViewportFooter>
          {empty ? <ChatSuggestions disabled={view.busy || view.pending || view.inaccessible} /> : null}
        </ThreadPrimitive.Viewport>
      </ThreadPrimitive.Root>
    </AssistantRuntimeProvider>
  );
}

function ChatEntrance() {
  return <div className="chat-entrance mx-auto flex w-full max-w-measure flex-col items-center gap-lg pb-xl pt-2xl text-center">
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
      <Button variant="outline" disabled={disabled} className="h-auto min-h-28 flex-col items-start gap-sm whitespace-normal p-lg text-left">
        <Icon size={18} className="text-pulse" aria-hidden="true" />
        <span>{title}</span><span className="text-label font-normal text-muted-foreground">{detail}</span>
      </Button>
    </ThreadPrimitive.Suggestion>)}
  </div>;
}

function renderMessage({message}: {message: MessageState}) {
  const user = message.role === MESSAGE_ROLE.user;
  const text = message.content.filter((part) => part.type === "text").map((part) => part.text).join("\n");
  const running = message.status?.type === "running";
  return <MessagePrimitive.Root className="py-md">
    <div className={user ? "ml-auto max-w-prose rounded-xl bg-muted px-lg py-md" : "max-w-prose space-y-sm"}>
      <p className="text-xs font-medium text-muted-foreground">{user ? "You" : "Fleet setup"}</p>
      {running && !text ? <output aria-label="Working" className="inline-flex items-center gap-sm text-label text-muted-foreground"><SignalSpinner className="text-pulse" />Working…</output>
        : <FleetReplyMarkdown text={text} streaming={running} />}
    </div>
  </MessagePrimitive.Root>;
}

function toThreadMessage(message: Message): ThreadMessageLike {
  const failure = message.failure;
  const recovery = failure ? FAILURE_COPY[failure.recovery_action] ?? "Refresh the durable status before continuing." : "";
  const pending = isRunning(message);
  const text = [message.text ?? "",
    message.role === MESSAGE_ROLE.assistant && failure ? `${failure.error_code}: ${recovery}` : "",
    message.role === MESSAGE_ROLE.assistant && message.state === MESSAGE_STATE.cancelled ? "Authoring stopped." : ""].filter(Boolean).join("\n\n");
  return {id: `${message.sequence}:${message.role}`, role: message.role, content: [{type: "text", text}], createdAt: new Date(message.created_at),
    ...(message.role === MESSAGE_ROLE.assistant ? {status: pending ? {type: "running"} : {type: "complete", reason: "stop"}} : {})};
}
