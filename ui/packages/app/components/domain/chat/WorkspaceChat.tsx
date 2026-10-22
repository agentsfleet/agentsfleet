"use client";

import type { DraftIdentity, DraftView } from "@/lib/api/draft-state";
import type { Schedule, Source } from "@/lib/api/draft-types";
import { useEffect, useState, useSyncExternalStore } from "react";
import Link from "next/link";
import nextDynamic from "next/dynamic";
import { useRouter } from "next/navigation";
import { Alert, Button, Skeleton } from "@agentsfleet/design-system";
import { stagePlatformLibraryAction } from "@/app/(dashboard)/admin/fleet-libraries/actions";
import { SetupUnavailable } from "@/lib/api/draft-request";
import { DraftClient } from "@/lib/api/drafts";
import { DraftCache } from "@/lib/api/draft-cache";
import { isRunning } from "@/lib/api/draft-types";
import { useCurrentUser } from "@/lib/auth/client";
import { useDraftSidebarBridge } from "@/components/layout/use-draft-sidebar-bridge";
import { ScheduleSync } from "./ScheduleSync";
import { ChatAvailability, ChatModelAvailability } from "./ChatAvailability";
import { FleetSetup } from "./FleetSetup";
import { DraftChatHeading } from "./DraftChatHeading";

type Props = {account: string; workspace: string; draft?: string; source?: Source; canPublishPlatform?: boolean; canManagePlatformModels?: boolean; onSaved?: () => void};
const ignore = (): void => undefined;
const LOADING_ROLE = "status";
const LOADING_SETUP_CHAT = "Loading setup chat";
const DraftConversation = nextDynamic(() => import("./DraftConversation").then((module) => module.DraftConversation), {
  ssr: false,
  loading: () => <Skeleton className="min-h-0 w-full flex-1" role={LOADING_ROLE} aria-label="Loading conversation" />,
});

export function WorkspaceChat(props: Props) {
  const {isLoaded, isSignedIn, userId} = useCurrentUser();
  if (!isLoaded) return <Skeleton className="h-80 w-full" role={LOADING_ROLE} aria-label="Loading your private chat" />;
  if (!isSignedIn || !userId) return <Alert variant="warning">Sign in again to open private fleet chat.</Alert>;
  const identity = {account: props.account, workspace: props.workspace, subject: userId};
  return <ChatSession key={JSON.stringify(identity)} {...props} identity={identity} />;
}

function ChatSession(props: Props & {identity: DraftIdentity}) {
  const [cache] = useState(() => new DraftCache());
  useEffect(() => () => cache.clear(), [cache]);
  return <OwnedChat key={JSON.stringify([props.draft, props.source])} {...props} cache={cache} />;
}

function OwnedChat({identity, draft, source, canPublishPlatform, canManagePlatformModels, onSaved, cache}: Props & {identity: DraftIdentity; cache: DraftCache}) {
  const [client, setClient] = useState<DraftClient | null>(null);
  const [opening, setOpening] = useState(true);
  const [reload, setReload] = useState<(() => Promise<void>) | null>(null);
  const [selection] = useState(() => ({identity, draft, source, canPublishPlatform}));
  useEffect(() => {
    const {identity: {account, workspace, subject}, draft, source, canPublishPlatform} = selection;
    const current = new DraftClient({account, workspace, subject}, canPublishPlatform ? async (body, operation) => {
      const result = await stagePlatformLibraryAction(body, operation);
      if (!result.ok) throw new SetupUnavailable(result.error, result.status ?? 502, {code: result.errorCode});
      return result.data;
    } : undefined);
    const unsubscribe = current.subscribe(() => {if (current.getSnapshot().inaccessible) cache.clear();});
    setClient(current);
    if (source) current.fresh(source);
    let disposed = false;
    const open = async (): Promise<void> => {
      const cached = draft ? cache.read(draft) : undefined;
      setOpening(!cached);
      const lists = current.initialize();
      void lists.catch(ignore);
      try {
        if (draft) await current.resume(draft, cached);
        else if (source) await current.refreshSetup();
        else await lists;
      } finally {if (!disposed) setOpening(false);}
    };
    setReload(() => open);
    void open().catch(ignore);
    return () => {disposed = true; unsubscribe(); cache.remember(current.getSnapshot()); current.dispose();};
  }, [selection, cache]);
  return client && reload ? <ChatSurface onSaved={onSaved} canManagePlatformModels={canManagePlatformModels} client={client} workspace={identity.workspace} subject={identity.subject} opening={opening} selected={Boolean(selection.draft || selection.source)} reload={reload} /> : <Skeleton className="min-h-0 w-full flex-1" role={LOADING_ROLE} aria-label={LOADING_SETUP_CHAT} />;
}

function ChatSurface({client, workspace, subject, opening, selected, reload, canManagePlatformModels, onSaved}: {canManagePlatformModels?: boolean; client: DraftClient; workspace: string; subject: string; opening: boolean; selected: boolean; reload: () => Promise<void>; onSaved?: () => void}) {
  const view = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  const router = useRouter();
  const savedFleet = view.fleet?.id ?? (view.savedSetupRevision ? view.draft?.linked_fleet_id : null);
  useEffect(() => {
    if (opening || view.busy || view.error || !savedFleet) return;
    if (onSaved) onSaved();
    else router.push(`/w/${workspace}/fleets/${savedFleet}`);
  }, [opening, view.busy, view.error, savedFleet, onSaved, router, workspace]);
  useDraftSidebarBridge(client, view, workspace, subject, view.messages.some(isRunning));
  const retrySchedule = (schedule: Schedule): void => { void client.syncSchedule(schedule).catch(ignore); };
  return <div className="ph-no-capture ph-no-record flex min-h-0 flex-1 flex-col gap-md" data-fleet-authoring>
    {view.error ? <ChatError client={client} view={view} workspace={workspace} reload={reload} /> : null}
    {view.notice ? <Alert>{view.notice}</Alert> : null}
    {opening ? <Skeleton className="min-h-0 w-full flex-1" role={LOADING_ROLE} aria-label={LOADING_SETUP_CHAT} /> : view.inaccessible || (selected && !view.draft && !view.preview) ? null : <>
    <DraftChatHeading client={client} view={view} />
    <ScheduleSync schedules={view.schedules} busy={view.busy} onRetry={retrySchedule} />
    <ChatModelAvailability canManagePlatformModels={canManagePlatformModels} client={client} view={view} workspace={workspace} />
    <DraftConversation client={client} view={view}>
      <FleetSetup client={client} view={view} workspace={workspace} />
    </DraftConversation>
    </>}
  </div>;
}

function ChatError({client, view, workspace, reload}: {client: DraftClient; view: DraftView; workspace: string; reload: () => Promise<void>}) {
  return <ChatAvailability title={view.inaccessible ? "Chat is unavailable" : "We couldn’t complete that"} actions={<>
        <Button size="sm" variant="secondary" disabled={view.busy} onClick={() => {
          if (view.inaccessible) window.location.reload();
          else void (view.draft || view.pending ? client.retry() : reload()).catch(ignore);
        }}>{view.inaccessible ? "Reload" : "Retry"}</Button>
        <Button size="sm" variant="ghost" asChild><Link href={`/w/${workspace}/fleets`}>Your fleets</Link></Button>
    </>}>{view.error}</ChatAvailability>;
}
