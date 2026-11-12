"use client";

import type { AppendMessage, ThreadMessageLike, MessageRuntimeState } from "@assistant-ui/react";
import type { DraftView } from "@/lib/api/draft-state";
import type { DraftClient } from "@/lib/api/drafts";
import type { ReactNode } from "react";
import { useMemo, useState, useCallback, useEffect } from "react";
import { AssistantRuntimeProvider, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";
import Link from "next/link";
import { GitPullRequestIcon, PuzzlePieceIcon, TerminalWindowIcon } from "@phosphor-icons/react/ssr";
import nextDynamic from "next/dynamic";
import { FleetConversationLayout } from "../FleetWorkSidebar";
import { BrandMark, Button, PageTitle, cn } from "@agentsfleet/design-system";
import { MESSAGE_ROLE, MESSAGE_STATE } from "@/lib/api/draft-types";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { CHAT_STARTERS } from "./chat-defaults";
import { ChatComposer } from "./ChatComposer";
import { ChatThread } from "./ChatThread";
import { createDraftThreadProjection } from "@/lib/api/draft-thread-messages";
import { setupActivity } from "@/lib/api/draft-activity";
import { ACTIVITY_KIND } from "@/lib/events/fleet-activity";
import { DraftMessage, DraftRetryProvider } from "./DraftMessage";
import { chatMessageText } from "@/lib/chat-message-text";
import { SetupApprovalSummary } from "./SetupApprovalSummary";
import { AuthoringPrompt } from "./AuthoringPrompt";

const SetupSummary = nextDynamic(() => import("./SetupSummary").then(module => module.SetupSummary), {ssr: false});

export function DraftConversation({client, view, workspace, subject, children, unfinished, instructionsDirty = false, onOpenFleet, modelsHref}: {client: DraftClient; view: DraftView; workspace: string; subject?: string; children?: ReactNode; unfinished?: ReactNode; instructionsDirty?: boolean; onOpenFleet?: () => void; modelsHref?: string}) {
  const recovery = chatRecovery(view);
  const [project] = useState(createDraftThreadProjection);
  const messages = useMemo(() => project(view), [project, view]);
  const anchor = setupAnchor(view);
  const saving = Boolean(view.saveOperation) && !view.error;
  const adapter = useMemo(() => ({
    messages, isRunning: recovery.pending || Boolean(view.optimistic) || saving, convertMessage: (message: ThreadMessageLike) => message,
    onNew: async (message: AppendMessage) => {
      const text = chatMessageText(message);
      await client.send(text);
    },
    onCancel: () => client.stop(),
  }), [client, messages, recovery.pending, view.optimistic, saving]);
  const runtime = useExternalStoreRuntime(adapter);
  const renderMessage = useCallback(({message}: {message: MessageRuntimeState}) => <><DraftMessage message={message} />{message.id === anchor ? children : null}</>, [anchor, children]);
  const empty = messages.length === 0 && !view.fleet && !view.draft && !view.preview;
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <AuthoringPrompt hasMessages={messages.length > 0} />
      <DraftRetryProvider client={client} disabled={view.busy || view.pending || recovery.pending || view.inaccessible || !view.model}>
      <SetupConversationLayout client={client} view={view} workspace={workspace} subject={subject}><ChatThread label="Fleet setup conversation" autoScroll={!empty} centered={empty}
        footer={<><ChatComposer empty={empty} client={client} view={view} workspace={workspace} onOpenFleet={onOpenFleet} modelsHref={modelsHref} canStop={recovery.canStop} blocked={instructionsDirty} dirty={instructionsDirty} />
          {empty ? <><ChatSuggestions disabled={view.busy || view.pending || view.inaccessible} /><p className="mb-xl mt-5 text-center text-eyebrow leading-label text-text-dim">No fleet selected. Start something new, or open your fleets from the rail.</p>{unfinished}</> : null}</>}>
        <div role="log" aria-live="off" className={cn("mx-auto flex w-full max-w-chat flex-1 flex-col", !empty && "py-3xl")}>
          {empty ? <ChatEntrance /> : null}
          {view.truncated ? <p className="text-sm text-muted-foreground">Showing the latest conversation messages. Earlier messages remain in this setup chat.
            {view.draft ? <Link className="ml-sm underline" href={`/w/${workspace}/fleets/new?draft=${view.draft.id}`}>Open full conversation</Link> : null}</p> : null}
          {view.conversationCursor || view.activityCursor ? <Button variant="ghost" size="sm" disabled={view.busy}
            onClick={() => {void client.loadEarlierConversations().catch(() => undefined);}}>Load earlier history</Button> : null}
          {(view.previousDrafts ?? []).filter(item => item.truncated).map(item => <Link key={item.id} className="text-sm underline" href={`/w/${workspace}/fleets/new?draft=${item.id}`}>Open full earlier conversation</Link>)}
          {anchor === null ? children : null}
          <ThreadPrimitive.Messages>{renderMessage}</ThreadPrimitive.Messages>
        </div>
      </ChatThread></SetupConversationLayout>
      </DraftRetryProvider>
    </AssistantRuntimeProvider>
  );
}
function SetupConversationLayout({client, view, workspace, subject, children}: {client: DraftClient; view: DraftView; workspace: string; subject?: string; children: ReactNode}) {
  const [refresh, setRefresh] = useState<{client: DraftClient; fleet: string; subject: string | undefined} | null>(null);
  useEffect(() => {
    if (!refresh) return;
    if (refresh.client !== client || refresh.fleet !== view.fleet?.id || refresh.subject !== subject || view.inaccessible) {
      setRefresh(null);
      return;
    }
    if (view.busy || client.getSnapshot().busy) return;
    setRefresh(null);
    void client.refreshFleetSetup(true).catch(() => undefined);
  }, [client, refresh, subject, view.busy, view.fleet?.id, view.inaccessible]);
  const resolved = (fleet: string) => setRefresh({client, fleet, subject});
  return <FleetConversationLayout work={view.draft || view.preview || view.fleet ? <div className="space-y-lg">
    <SetupApprovalSummary view={view} workspace={workspace} subject={subject} onResolved={resolved} />
    <SetupSummary definition={view.draft?.candidate?.definition ?? view.preview?.definition ?? view.fleet?.definition} requirements={view.draft?.review?.requirements} />
  </div> : undefined}>
    {children}
  </FleetConversationLayout>;
}

function setupAnchor(view: DraftView): string | null {
  const saved = (view.activities ?? []).find(row => {
    const activity = setupActivity(row);
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
  return <div className="content-entrance flex w-full flex-col items-center text-center">
    <span className="mb-2xl flex size-11 items-center justify-center rounded-control bg-brand-tile"><BrandMark className="size-6 text-brand-ray" aria-hidden="true" /></span>
    <PageTitle className="mb-lg text-new-fleet-title leading-new-fleet-title tracking-new-fleet-title">What should your fleet do?</PageTitle>
    <p className="text-new-fleet-description text-muted-foreground">Give it a goal. Follow the work. Step in when you’re needed.</p>
  </div>;
}

function ChatSuggestions({ disabled }: { disabled: boolean }) {
  return <div className="mt-5 flex w-full flex-wrap justify-start gap-lg sm:justify-center">
    {CHAT_STARTERS.map(({ title, prompt }, index) => <ThreadPrimitive.Suggestion key={title} prompt={prompt} send={false} asChild>
      <Button variant="outline" size="sm" disabled={disabled} className="min-h-9.5 rounded-full text-eyebrow font-semibold text-muted-foreground">{index === 0 ? <TerminalWindowIcon size={16} /> : index === 1 ? <GitPullRequestIcon size={16} /> : <PuzzlePieceIcon size={16} />}{title}</Button>
    </ThreadPrimitive.Suggestion>)}
  </div>;
}
