"use client";

import type { DraftIdentity, DraftView } from "@/lib/api/draft-state";
import type { Source } from "@/lib/api/draft-types";
import { useEffect, useState, useSyncExternalStore } from "react";
import Link from "next/link";
import nextDynamic from "next/dynamic";
import { Alert, AlertTitle, Button, DashboardPanel, DashboardPanelDescription, Skeleton } from "@agentsfleet/design-system";
import { CircleAlertIcon } from "lucide-react";
import { stagePlatformLibraryAction } from "@/app/(dashboard)/admin/fleet-libraries/actions";
import { SetupUnavailable } from "@/lib/api/draft-request";
import { DraftClient } from "@/lib/api/drafts";
import { isRunning } from "@/lib/api/draft-types";
import { useCurrentUser } from "@/lib/auth/client";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { useDraftSidebarBridge } from "@/components/layout/DraftSidebarHistory";
import { SetupProgress } from "./SetupProgress";

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
  const review = (): void => {
    setReviewOpen(true);
    captureProductEvent(EVENTS.draft_review_opened, {workspace_id: workspace, draft_id: view.draft?.id});
    void client.review().catch(ignore);
  };
  return <div className="ph-no-capture ph-no-record flex min-h-0 flex-1 flex-col gap-md" data-fleet-authoring>
    {view.error ? <ChatError client={client} view={view} workspace={workspace} /> : null}
    {view.notice ? <Alert>{view.notice}</Alert> : null}
    {view.models.length === 0 && !view.busy && !view.inaccessible && !view.error ? <Alert variant="warning">Add a model to this workspace to start chatting.
      <Button variant="link" asChild><Link href={`/w/${workspace}/settings/models`}>Open model settings</Link></Button></Alert> : null}
    <DraftConversation client={client} view={view} onReview={review} />
    {view.fleet ? <SetupProgress fleet={view.fleet} workspace={workspace} busy={view.busy} onContinue={() => { void client.retry().catch(ignore); }} /> : null}
    {reviewOpen ? <ReviewSetup client={client} view={view} workspace={workspace} open={reviewOpen} onOpenChange={setReviewOpen} /> : null}
  </div>;
}

function ChatError({client, view, workspace}: {client: DraftClient; view: DraftView; workspace: string}) {
  return <DashboardPanel role="alert" padding="compact" className="mx-auto flex w-full max-w-measure items-start gap-md">
    <CircleAlertIcon size={18} className="mt-xs shrink-0 text-warning" aria-hidden="true" />
    <div className="min-w-0 flex-1 space-y-sm">
      <AlertTitle>{view.inaccessible ? "Chat is unavailable" : "We couldn’t complete that"}</AlertTitle>
      <DashboardPanelDescription>{view.error}</DashboardPanelDescription>
      <div className="flex flex-wrap items-center gap-sm">
        <Button size="sm" variant="secondary" disabled={view.busy} onClick={() => {
          if (view.inaccessible) window.location.reload();
          else void (view.draft || view.pending ? client.retry() : client.initialize()).catch(ignore);
        }}>{view.inaccessible ? "Reload" : "Retry"}</Button>
        <Button size="sm" variant="ghost" asChild><Link href={`/w/${workspace}/fleets`}>Your fleets</Link></Button>
      </div>
    </div>
  </DashboardPanel>;
}
