"use client";

import type { DraftIdentity, DraftView } from "@/lib/api/draft-state";
import type { Schedule, Source } from "@/lib/api/draft-types";
import { useEffect, useState, useSyncExternalStore } from "react";
import Link from "next/link";
import nextDynamic from "next/dynamic";
import { Alert, Button, Skeleton } from "@agentsfleet/design-system";
import { stagePlatformLibraryAction } from "@/app/(dashboard)/admin/fleet-libraries/actions";
import { SetupUnavailable } from "@/lib/api/draft-request";
import { DraftClient } from "@/lib/api/drafts";
import { isRunning } from "@/lib/api/draft-types";
import { useCurrentUser } from "@/lib/auth/client";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { useDraftSidebarBridge } from "@/components/layout/DraftSidebarHistory";
import { SetupProgress } from "./SetupProgress";
import { ScheduleSync } from "./ScheduleSync";
import { ChatAvailability, ChatModelAvailability } from "./ChatAvailability";

type Props = {account: string; workspace: string; draft?: string; source?: Source; canPublishPlatform?: boolean};
const ignore = (): void => undefined;
const DraftConversation = nextDynamic(() => import("./DraftConversation").then((module) => module.DraftConversation), {
  ssr: false,
  loading: () => <Skeleton className="min-h-0 w-full flex-1" aria-label="Loading conversation" />,
});
const ReviewSetup = nextDynamic(() => import("./ReviewSetup").then((module) => module.ReviewSetup));

export function WorkspaceChat(props: Props) {
  const {isLoaded, isSignedIn, userId} = useCurrentUser();
  if (!isLoaded) return <Skeleton className="h-80 w-full" aria-label="Loading your private chat" />;
  if (!isSignedIn || !userId) return <Alert variant="warning">Sign in again to open private fleet chat.</Alert>;
  const identity = {account: props.account, workspace: props.workspace, subject: userId};
  return <OwnedChat key={JSON.stringify(identity)} {...props} identity={identity} />;
}

function OwnedChat({identity, draft, source, canPublishPlatform}: Props & {identity: DraftIdentity}) {
  const [client, setClient] = useState<DraftClient | null>(null);
  const {account, workspace, subject} = identity;
  useEffect(() => {
    const current = new DraftClient({account, workspace, subject}, canPublishPlatform ? async (body, operation) => {
      const result = await stagePlatformLibraryAction(body, operation);
      if (!result.ok) throw new SetupUnavailable(result.error, result.status ?? 502, {code: result.errorCode});
      return result.data;
    } : undefined);
    setClient(current);
    if (source) current.fresh(source);
    void current.initialize().then(async () => { if (draft) await current.resume(draft); }).catch(ignore);
    return () => current.dispose();
  }, [account, workspace, subject, draft, source, canPublishPlatform]);
  return client ? <ChatSurface client={client} workspace={identity.workspace} subject={identity.subject} /> : <Skeleton className="h-80 w-full" />;
}

function ChatSurface({client, workspace, subject}: {client: DraftClient; workspace: string; subject: string}) {
  const view = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  const [reviewOpen, setReviewOpen] = useState(false);
  useDraftSidebarBridge(client, view, workspace, subject, view.messages.some(isRunning));
  const retrySchedule = (schedule: Schedule): void => { void client.syncSchedule(schedule).catch(ignore); };
  const review = (): void => {
    setReviewOpen(true);
    captureProductEvent(EVENTS.draft_review_opened, {workspace_id: workspace, draft_id: view.draft?.id});
    void client.review().catch(ignore);
  };
  return <div className="ph-no-capture ph-no-record flex min-h-0 flex-1 flex-col gap-md" data-fleet-authoring>
    {view.error ? <ChatError client={client} view={view} workspace={workspace} /> : null}
    {view.notice ? <Alert>{view.notice}</Alert> : null}
    <ScheduleSync schedules={view.schedules} busy={view.busy} onRetry={retrySchedule} />
    <ChatModelAvailability client={client} view={view} workspace={workspace} />
    <DraftConversation client={client} view={view} onReview={review} />
    {view.fleet ? <SetupProgress fleet={view.fleet} workspace={workspace} busy={view.busy}
      onContinue={() => { void client.retry().catch(ignore); }} onRetrySchedule={retrySchedule} /> : null}
    {reviewOpen ? <ReviewSetup client={client} view={view} workspace={workspace} open={reviewOpen} onOpenChange={setReviewOpen} /> : null}
  </div>;
}

function ChatError({client, view, workspace}: {client: DraftClient; view: DraftView; workspace: string}) {
  return <ChatAvailability title={view.inaccessible ? "Chat is unavailable" : "We couldn’t complete that"} actions={<>
        <Button size="sm" variant="secondary" disabled={view.busy} onClick={() => {
          if (view.inaccessible) window.location.reload();
          else void (view.draft || view.pending ? client.retry() : client.initialize()).catch(ignore);
        }}>{view.inaccessible ? "Reload" : "Retry"}</Button>
        <Button size="sm" variant="ghost" asChild><Link href={`/w/${workspace}/fleets`}>Your fleets</Link></Button>
    </>}>{view.error}</ChatAvailability>;
}
