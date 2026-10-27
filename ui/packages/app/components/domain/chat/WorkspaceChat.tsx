"use client";

import type { DraftIdentity, DraftView } from "@/lib/api/draft-state";
import type { Source } from "@/lib/api/draft-types";
import type { ReactNode } from "react";
import { useEffect, useState, useSyncExternalStore } from "react";
import Link from "next/link";
import nextDynamic from "next/dynamic";
import { Alert, Button, Skeleton } from "@agentsfleet/design-system";
import { stagePlatformLibraryAction } from "@/app/(dashboard)/admin/fleet-libraries/actions";
import { SetupUnavailable } from "@/lib/api/draft-request";
import { DraftClient } from "@/lib/api/drafts";
import { DraftCache } from "@/lib/api/draft-cache";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { SOURCE_KIND } from "@/lib/api/draft-vocabulary";
import { appliedNotice, isRunning } from "@/lib/api/draft-types";
import { useCurrentUser } from "@/lib/auth/client";
import { useDraftSidebarBridge } from "@/components/layout/use-draft-sidebar-bridge";
import { ChatAvailability, ChatModelAvailability } from "./ChatAvailability";
import { FleetSetup } from "./FleetSetup";
import { DraftChatHeading } from "./DraftChatHeading";
import { UnfinishedSetups } from "./UnfinishedSetups";

type Props = {account: string; workspace: string; draft?: string; source?: Source; fleetPage?: boolean; menuItems?: ReactNode; canPublishPlatform?: boolean; canManagePlatformModels?: boolean};
const ignore = (): void => undefined;
const LOADING_ROLE = "status";
const LOADING_SETUP_CHAT = "Loading setup chat";
const DraftConversation = nextDynamic(() => import("./DraftConversation").then((module) => module.DraftConversation), {
  ssr: false,
  loading: () => <Skeleton className="min-h-0 w-full flex-1" role={LOADING_ROLE} aria-label="Loading conversation" />,
});

export function WorkspaceChat(props: Props) {
  const {isLoaded, isSignedIn, userId} = useCurrentUser();
  if (!isLoaded) return <Skeleton className="h-80 w-full" role={LOADING_ROLE} aria-label="Loading your setup chat" />;
  if (!isSignedIn || !userId) return <Alert variant="warning">Sign in again to open fleet setup chat.</Alert>;
  const identity = {account: props.account, workspace: props.workspace, subject: userId};
  return <ChatSession key={JSON.stringify(identity)} {...props} identity={identity} />;
}

function ChatSession(props: Props & {identity: DraftIdentity}) {
  const [cache] = useState(() => new DraftCache());
  useEffect(() => () => cache.clear(), [cache]);
  const sourceKey = props.source?.kind === SOURCE_KIND.fleet ? {kind: props.source.kind, fleet_id: props.source.fleet_id} : props.source;
  return <OwnedChat key={JSON.stringify([props.draft, sourceKey])} {...props} cache={cache} />;
}

function OwnedChat({identity, draft, source, canPublishPlatform, canManagePlatformModels, fleetPage, menuItems, cache}: Props & {identity: DraftIdentity; cache: DraftCache}) {
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
      setOpening(!cached && !current.getSnapshot().preview);
      const lists = current.initialize();
      void lists.catch(ignore);
      try {
        if (draft) await current.resume(draft, cached);
        else if (source?.kind === SOURCE_KIND.fleet) await current.refreshFleetSetup();
        else if (source) await current.refreshSetup();
        else await lists;
      } finally {if (!disposed) setOpening(false);}
    };
    setReload(() => open);
    void open().catch(ignore);
    return () => {disposed = true; unsubscribe(); cache.remember(current.getSnapshot()); current.dispose();};
  }, [selection, cache]);
  return client && reload ? <ChatSurface source={source} fleetPage={fleetPage} menuItems={menuItems} canManagePlatformModels={canManagePlatformModels} client={client} workspace={identity.workspace} subject={identity.subject} opening={opening} selected={Boolean(selection.draft || selection.source)} reload={reload} /> : <Skeleton className="min-h-0 w-full flex-1" role={LOADING_ROLE} aria-label={LOADING_SETUP_CHAT} />;
}

function ChatSurface({client, source, workspace, subject, opening, selected, reload, canManagePlatformModels, fleetPage, menuItems}: {source?: Source; fleetPage?: boolean; menuItems?: ReactNode; canManagePlatformModels?: boolean; client: DraftClient; workspace: string; subject: string; opening: boolean; selected: boolean; reload: () => Promise<void>}) {
  const view = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  const [instructionsDirty, setInstructionsDirty] = useState(false);
  useEffect(() => {
    if (opening || view.busy || view.pending || view.error || view.draft || view.inaccessible || view.savedSetupRevision
      || source?.kind !== SOURCE_KIND.fleet || !view.preview || view.preview.setupRevision === source.setup_revision) return;
    client.fresh(source);
    void client.refreshFleetSetup().catch(ignore);
  }, [client, source, opening, view.busy, view.pending, view.error, view.draft, view.inaccessible, view.savedSetupRevision, view.preview]);
  useDraftSidebarBridge(client, view, workspace, subject, view.messages.some(isRunning));
  return <div className="ph-no-capture ph-no-record flex min-h-0 flex-1 flex-col" data-fleet-authoring>
    {view.error ? <ChatError client={client} view={view} source={source} workspace={workspace} reload={reload} onStartOver={() => setInstructionsDirty(false)} /> : null}
    {view.notice && view.notice !== appliedNotice([]) ? <Alert>{view.notice}</Alert> : null}
    {opening ? <Skeleton className="min-h-0 w-full flex-1" role={LOADING_ROLE} aria-label={LOADING_SETUP_CHAT} /> : view.inaccessible || (selected && !view.draft && !view.preview) ? null : <>
    <DraftChatHeading client={client} view={view} workspace={workspace} fleetPage={fleetPage} menuItems={menuItems} onDirtyChange={setInstructionsDirty} />
    <ChatModelAvailability canManagePlatformModels={canManagePlatformModels} client={client} view={view} workspace={workspace} />
    <DraftConversation client={client} view={view} instructionsDirty={instructionsDirty} unfinished={<UnfinishedSetups workspace={workspace} onReload={() => client.initialize()} />}>
      <FleetSetup client={client} view={view} workspace={workspace} />
    </DraftConversation>
    </>}
  </div>;
}

function ChatError({client, view, source, workspace, reload, onStartOver}: {client: DraftClient; view: DraftView; source?: Source; workspace: string; reload: () => Promise<void>; onStartOver: () => void}) {
  return <ChatAvailability title={view.inaccessible ? "Chat is unavailable" : "We couldn’t complete that"} actions={<>
        <Button size="sm" variant="secondary" disabled={view.busy} onClick={() => {
          if (view.inaccessible) window.location.reload();
          else void (view.draft || view.pending ? client.retry() : reload()).catch(ignore);
        }}>{view.inaccessible ? "Reload" : "Retry"}</Button>
        {!view.inaccessible && source?.kind === SOURCE_KIND.fleet ? <Button size="sm" variant="secondary" disabled={view.busy || view.pending || chatRecovery(view).pending} onClick={() => {
          client.fresh(source); onStartOver(); void reload().catch(ignore);
        }}>Start over from current setup</Button> : null}
        <Button size="sm" variant="ghost" asChild><Link href={`/w/${workspace}/fleets`}>Your fleets</Link></Button>
    </>}>{view.error}</ChatAvailability>;
}
