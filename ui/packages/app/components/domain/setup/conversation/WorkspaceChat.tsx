"use client";

import type { DraftIdentity, DraftView } from "@/lib/api/draft-state";
import type { Source } from "@/lib/api/draft-types";
import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import nextDynamic from "next/dynamic";
import { useRouter } from "next/navigation";
import { Alert, Button, Skeleton } from "@agentsfleet/design-system";
import { DraftClient } from "@/lib/api/drafts";
import { DraftCache } from "@/lib/api/draft-cache";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { SOURCE_KIND } from "@/lib/api/draft-vocabulary";
import { appliedNotice, isRunning } from "@/lib/api/draft-types";
import { useCurrentUser } from "@/lib/auth/client";
import { useDraftSidebarBridge } from "@/components/layout/use-draft-sidebar-bridge";
import { DraftChatHeading } from "./DraftChatHeading";

type Props = {account: string; workspace: string; draft?: string; source?: Source; fleetPage?: boolean; menuItems?: ReactNode};
type OwnedSession = {readonly kind: "starting"} | {readonly kind: "loading" | "ready"; readonly client: DraftClient; readonly reload: () => Promise<void>};
const ignore = (): void => undefined;
const LOADING_ROLE = "status";
const LOADING_SETUP_CHAT = "Loading setup chat";
const FleetSetup = nextDynamic(() => import("./FleetSetup").then(module => module.FleetSetup), {ssr: false});
const UnfinishedSetups = nextDynamic(() => import("./UnfinishedSetups").then(module => module.UnfinishedSetups), {ssr: false});
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

function OwnedChat({identity, draft, source, fleetPage, menuItems, cache}: Props & {identity: DraftIdentity; cache: DraftCache}) {
  const [session, setSession] = useState<OwnedSession>({kind: "starting"});
  const [selection] = useState(() => ({identity, draft, source}));
  useEffect(() => {
    const {identity: {account, workspace, subject}, draft, source} = selection;
    const current = new DraftClient({account, workspace, subject});
    const unsubscribe = current.subscribe(() => {if (current.getSnapshot().inaccessible) cache.clear();});
    if (source) current.fresh(source);
    let disposed = false;
    const open = async (): Promise<void> => {
      const cached = draft ? cache.read(draft) : undefined;
      const opening = Boolean(draft || source) && !cached && !current.getSnapshot().preview;
      setSession({kind: opening ? "loading" : "ready", client: current, reload: open});
      void current.initialize().catch(ignore);
      try {
        if (draft) await current.resume(draft, cached);
        else if (source?.kind === SOURCE_KIND.fleet) await current.refreshFleetSetup();
        else if (source) await current.refreshSetup();
      } finally {if (!disposed && opening) setSession({kind: "ready", client: current, reload: open});}
    };
    void open().catch(ignore);
    return () => {disposed = true; unsubscribe(); cache.remember(current.getSnapshot()); current.dispose();};
  }, [selection, cache]);
  const initialSourceRevision = selection.source?.kind === SOURCE_KIND.fleet ? selection.source.setup_revision : undefined;
  return session.kind === "starting" ? <Skeleton className="min-h-0 w-full flex-1" role={LOADING_ROLE} aria-label={LOADING_SETUP_CHAT} />
    : <ChatSurface initialSourceRevision={initialSourceRevision} source={source} draftId={draft} fleetPage={fleetPage} menuItems={menuItems} client={session.client} workspace={identity.workspace} subject={identity.subject} opening={session.kind === "loading"} selected={Boolean(selection.draft || selection.source)} reload={session.reload} />;
}

function ChatSurface({client, source, draftId, initialSourceRevision, workspace, subject, opening, selected, reload, fleetPage, menuItems}: {source?: Source; draftId?: string; initialSourceRevision?: string; fleetPage?: boolean; menuItems?: ReactNode; client: DraftClient; workspace: string; subject: string; opening: boolean; selected: boolean; reload: () => Promise<void>}) {
  const router = useRouter();
  const openFleet = useCallback(() => router.refresh(), [router]);
  const view = useSyncExternalStore(client.subscribe, client.getLayoutSnapshot, client.getLayoutSnapshot);
  useFleetSetupUrl(workspace, source, draftId, view, opening, fleetPage);
  const handledSourceRevision = useRef(initialSourceRevision);
  const turnPending = chatRecovery(view).pending;
  useEffect(() => {
    if (opening || view.busy || view.pending || turnPending || view.error || view.inaccessible
      || source?.kind !== SOURCE_KIND.fleet || !view.preview || handledSourceRevision.current === source.setup_revision) return;
    handledSourceRevision.current = source.setup_revision;
    if (view.preview.setupRevision === source.setup_revision || view.savedSetupRevision === source.setup_revision) return;
    client.fresh(source);
    void client.refreshFleetSetup().catch(ignore);
  }, [client, source, opening, turnPending, view.busy, view.pending, view.error, view.draft, view.inaccessible, view.savedSetupRevision, view.preview]);
  useDraftSidebarBridge(client, view, workspace, subject, view.messages.some(isRunning));
  const unavailable = view.inaccessible || (selected && !view.draft && !view.preview);
  const retry = async (): Promise<void> => {
    const current = client.getSnapshot();
    if (current.inaccessible) {window.location.reload();return;}
    if (current.busy || chatRecovery(current).pending) return;
    if (current.pending || current.draft || current.fleet) await client.retry();
    else if (source?.kind === SOURCE_KIND.fleet) await client.refreshFleetSetup(false, true);
    else await reload();
  };
  return <div className="ph-no-capture ph-no-record flex min-h-0 flex-1 flex-col" data-fleet-setup>
    {unavailable && !opening && view.error ? <div role="alert" aria-label="Chat recovery" className="flex flex-wrap items-center gap-md px-xl py-md text-label">
      <p className="text-destructive">{view.error}</p>
      <Button size="compact" variant="link" disabled={view.busy} onClick={() => {void retry().catch(ignore);}}>{view.inaccessible ? "Reload" : "Retry"}</Button>
    </div> : null}
    {view.notice && view.notice !== appliedNotice([]) ? <Alert>{view.notice}</Alert> : null}
    {opening ? <Skeleton className="min-h-0 w-full flex-1" role={LOADING_ROLE} aria-label={LOADING_SETUP_CHAT} /> : unavailable ? null : <>
    <DraftChatHeading client={client} view={view} workspace={workspace} fleetPage={fleetPage} returnToFleet={source?.kind === SOURCE_KIND.fleet ? source.fleet_id : undefined} menuItems={menuItems} />
    <DraftConversation client={client} view={view} workspace={workspace} subject={subject} modelsHref={`/w/${workspace}/settings/models`} onOpenFleet={fleetPage ? openFleet : undefined} onRequestRetry={retry} unfinished={<UnfinishedSetups workspace={workspace} onReload={() => client.initialize()} />}>
      {view.fleet || view.draft?.candidate || view.preview ? <FleetSetup client={client} view={view} workspace={workspace} /> : null}
    </DraftConversation>
    </>}
  </div>;
}

function useFleetSetupUrl(workspace: string, source: Source | undefined, draftId: string | undefined, view: DraftView, opening: boolean, fleetPage?: boolean): void {
  const router = useRouter();
  const replaced = useRef<string | undefined>(undefined);
  const savedDraftId = view.draft?.id;
  useEffect(() => {
    if (fleetPage || opening || view.busy || view.inaccessible || view.error) return;
    let query: URLSearchParams;
    if (source?.kind === SOURCE_KIND.fleet) {
      const revision = view.savedSetupRevision ?? view.preview?.setupRevision;
      if (!revision || view.fleet?.id !== source.fleet_id || BigInt(revision) <= BigInt(source.setup_revision)) return;
      query = new URLSearchParams({fleet: source.fleet_id, setup_revision: revision});
    } else {
      if (source || draftId || !savedDraftId || !view.savedSetupRevision) return;
      query = new URLSearchParams({draft: savedDraftId});
    }
    const url = `/w/${workspace}/fleets/new?${query}`;
    if (replaced.current === url) return;
    replaced.current = url;
    if (source) router.replace(url, {scroll: false});
    else window.history.replaceState(null, "", url);
  }, [router, workspace, source, draftId, opening, fleetPage, view.busy, view.inaccessible, view.error, view.fleet?.id, view.preview?.setupRevision, view.savedSetupRevision, savedDraftId]);
}
