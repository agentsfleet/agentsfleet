"use client";

import { Fragment, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import {useRecentPage} from "@/lib/api/use-recent-page";
import {DraftCardsSchema, type DraftCard} from "@/lib/api/draft-types";
import type {DraftSidebarState} from "./draft-sidebar-state";
import {sidebarDrafts, CHAT_ACTIVITY, type SidebarDraft} from "@/lib/api/draft-summary";
import Link from "next/link";
import { NavItem, IconAction, Button } from "@agentsfleet/design-system";
import { NotePencilIcon, TrashIcon } from "@phosphor-icons/react/ssr";
import { FleetListSchema, type NativeFleetSummary } from "@/lib/api/fleet-detail";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import type { RecentCollection } from "@/lib/api/recent-collection";
import { FleetLifecycleDialog } from "./FleetLifecycleDialog";

export function RecentFleetList({workspace, onNavigate, history = null}: {workspace: string; onNavigate: () => void; history?: DraftSidebarState | null}) {
  const collection = useRecentPage(`/live/v1/workspaces/${encodeURIComponent(workspace)}/fleets`, FleetListSchema);
  const drafts = useRecentPage(`/live/v1/workspaces/${encodeURIComponent(workspace)}/drafts`, DraftCardsSchema, !history);
  return collection && drafts ? <FleetItems history={history} drafts={drafts} collection={collection} workspace={workspace} onNavigate={onNavigate} /> : null;
}

function FleetItems({collection, drafts, history, workspace, onNavigate}: {history: DraftSidebarState | null; drafts: RecentCollection<DraftCard>; collection: RecentCollection<NativeFleetSummary>; workspace: string; onNavigate: () => void}) {
  const page = useSyncExternalStore(collection.subscribe, collection.getSnapshot, collection.getSnapshot);
  const setups = useSyncExternalStore(drafts.subscribe, drafts.getSnapshot, drafts.getSnapshot);
  const setupsLoading = history ? history.status === "loading" : setups.loading;
  const openSetups = history ? history.cards.filter(card => card.activity !== CHAT_ACTIVITY.saved) : sidebarDrafts(setups.items.filter(card => card.state !== "closed"), null);
  const edits = Map.groupBy(openSetups, card => card.linkedFleet);
  const fleetIds = new Set(page.items.map(fleet => fleet.id));
  const query = useSearchParams();
  const currentDraft = query.get("draft") ?? history?.current ?? null;
  const pathname = usePathname();
  const router = useRouter();
  const [selected, setSelected] = useState<NativeFleetSummary | null>(null);
  const previousPath = useRef(pathname);
  const savedFleet = history?.cards.find(card => card.id === history.current && card.activity === CHAT_ACTIVITY.saved)?.linkedFleet;
  const previousSave = useRef(savedFleet);
  useEffect(() => {if (previousPath.current !== pathname) {previousPath.current = pathname; void collection.load(); if (!history) void drafts.load();}}, [collection, drafts, history, pathname]);
  useEffect(() => {
    if (savedFleet && previousSave.current !== savedFleet) void collection.load();
    previousSave.current = savedFleet;
  }, [collection, savedFleet]);
  const removed = (id: string): void => {
    collection.remove(id); setSelected(null);
    if (pathname.includes(`/fleets/${id}`)) router.replace(`/w/${workspace}/fleets/new`);
    else router.refresh();
  };
  const setupLink = (card: SidebarDraft) => renderSetupLink(card, workspace, pathname.endsWith("/fleets/new") && currentDraft === card.id, onNavigate);
  return <>
    {page.items.map(fleet => {
      const linked = edits.get(fleet.id) ?? [];
      const archived = fleet.status === AGENTSFLEET_STATUS.ARCHIVED;
      const primary = linked.find(card => card.id === currentDraft) ?? linked[0];
      const edit = archived ? undefined : fleetEditLink(workspace, fleet.id, primary, pathname, currentDraft, query.get("fleet"));
      return <Fragment key={fleet.id}>
        <FleetRow fleet={fleet} workspace={workspace} pathname={pathname} edit={edit} onNavigate={onNavigate}
          deleting={selected !== null} onDelete={() => setSelected(fleet)} />
        {archived ? null : linked.filter(card => card !== primary).map(setupLink)}
      </Fragment>;
    })}
    {openSetups.filter(card => !card.linkedFleet || !fleetIds.has(card.linkedFleet)).map(setupLink)}

    {!history && setups.error ? <Button variant="link" size="sm" disabled={setupsLoading} onClick={() => {void drafts.load();}}>Retry loading setups</Button> : null}
    {(history ? history.more : setups.more) ? <Button variant="ghost" size="sm" disabled={setupsLoading} onClick={() => {if (history) void history.more?.(); else void drafts.load(true);}}>Load more setups</Button> : null}
    {page.loading && !page.items.length ? <output className="px-md text-label text-muted-foreground">Loading fleets…</output> : null}
    {!page.loading && !page.error && !page.items.length && !openSetups.length && !setupsLoading ? <p className="px-md text-label text-muted-foreground">Saved fleets will appear here.</p> : null}
    {page.error ? <output className="px-md py-sm text-label text-muted-foreground">{page.error} <Button variant="link" size="sm" disabled={page.loading} onClick={() => {void collection.load();}}>Refresh</Button></output> : null}
    {page.more ? <Button variant="ghost" size="sm" disabled={page.loading} onClick={() => {void collection.load(true);}}>Load more fleets</Button> : null}
    {selected ? <FleetLifecycleDialog action="delete" fleet={selected} workspace={workspace} onRemoved={removed} onClose={() => setSelected(null)} /> : null}
  </>;
}

function renderSetupLink(card: SidebarDraft, workspace: string, active: boolean, onNavigate: () => void) {
  const href = `/w/${workspace}/fleets/new?draft=${card.id}`;
  return <NavItem asChild key={href} appearance="list" active={active} onClick={onNavigate} className="min-w-0 font-medium">
    <Link href={href} prefetch={false} title={card.title}>
      <span className="min-w-0 flex-1 truncate">{card.title}</span><NotePencilIcon size={14} className="shrink-0 text-text-dim" aria-hidden="true" />
    </Link>
  </NavItem>;
}

function fleetEditLink(workspace: string, fleetId: string, draft: SidebarDraft | undefined, pathname: string,
  currentDraft: string | null, selectedFleet: string | null) {
  const setupPath = `/w/${workspace}/fleets/${fleetId}/setup`;
  const setupPage = pathname.endsWith("/fleets/new");
  if (draft) return {href: `/w/${workspace}/fleets/new?draft=${draft.id}`, selected: setupPage && currentDraft === draft.id, title: draft.title};
  return {href: setupPath, selected: pathname === setupPath || setupPage && selectedFleet === fleetId};
}

function FleetRow({fleet, workspace, pathname, edit, onNavigate, deleting, onDelete}: {
  fleet: NativeFleetSummary; workspace: string; pathname: string; onNavigate: () => void;
  edit: {href: string; selected: boolean; title?: string | undefined} | undefined;
  deleting: boolean; onDelete: () => void;
}) {
  return <div className="group flex min-w-0 items-center">
    <NavItem asChild appearance="list" active={pathname === `/w/${workspace}/fleets/${fleet.id}`} className="min-w-0 flex-1" onClick={onNavigate}>
      <Link href={`/w/${workspace}/fleets/${fleet.id}`} prefetch={false} title={`${fleet.name} · ${fleet.status}`}>
        <span className="min-w-0 flex-1 truncate font-semibold text-foreground">{fleet.name}</span>
        {fleet.status === AGENTSFLEET_STATUS.ARCHIVED ? <span className="shrink-0 text-eyebrow text-text-dim">Archived</span> : null}
      </Link>
    </NavItem>
    {edit ? <NavItem asChild appearance="list" active={edit.selected} className="w-9 shrink-0 justify-center px-0 pointer-coarse:w-11" onClick={onNavigate}>
      <Link href={edit.href} prefetch={false} aria-label={`Edit ${fleet.name}`} title={edit.title ?? `Edit ${fleet.name}`}>
        <NotePencilIcon size={14} aria-hidden="true" />
      </Link>
    </NavItem> : null}
    <IconAction revealOnRowFocus variant="ghost" label={`Permanently delete fleet: ${fleet.name}`} disabled={deleting}
      className="shrink-0"
      onClick={onDelete}><TrashIcon size={14} aria-hidden="true" /></IconAction>
  </div>;
}
