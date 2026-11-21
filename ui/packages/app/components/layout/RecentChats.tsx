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
  const currentDraft = query.get("draft") ?? history?.current;
  const pathname = usePathname();
  const router = useRouter();
  const [selected, setSelected] = useState<NativeFleetSummary | null>(null);
  const previousPath = useRef(pathname);
  useEffect(() => {if (previousPath.current !== pathname) {previousPath.current = pathname; void collection.load(); if (!history) void drafts.load();}}, [collection, drafts, history, pathname]);
  const removed = (id: string): void => {
    collection.remove(id); setSelected(null);
    if (pathname.includes(`/fleets/${id}`)) router.replace(`/w/${workspace}/fleets/new`);
    else router.refresh();
  };
  function setupLink(title: string, card?: SidebarDraft, fleetId?: string) {
    const href = card ? `/w/${workspace}/fleets/new?draft=${card.id}` : `/w/${workspace}/fleets/${fleetId}/setup`;
    return <NavItem asChild key={href} active={pathname.endsWith("/fleets/new") && (card ? currentDraft === card.id : query.get("fleet") === fleetId)}
      onClick={onNavigate} className="mb-xs min-w-0 gap-sm pl-xl text-label text-muted-foreground hover:bg-muted">
      <Link href={href} prefetch={false} title={card?.title ?? title}>
        <NotePencilIcon size={14} aria-hidden="true" /><span className="truncate">{title}</span>
      </Link>
    </NavItem>;
  }
  return <>
    {page.items.map(fleet => <Fragment key={fleet.id}>
      <FleetRow fleet={fleet} workspace={workspace} pathname={pathname} onNavigate={onNavigate}
        deleting={selected !== null} onDelete={() => setSelected(fleet)} />
      {fleet.status !== AGENTSFLEET_STATUS.ARCHIVED && (edits.get(fleet.id)?.map(card => setupLink(`Edit ${fleet.name}`, card))
        ?? setupLink(`Edit ${fleet.name}`, undefined, fleet.id))}
    </Fragment>)}
    {openSetups.filter(card => !card.linkedFleet || !fleetIds.has(card.linkedFleet)).map(card => setupLink(card.title, card))}

    {!history && setups.error ? <Button variant="link" size="sm" disabled={setupsLoading} onClick={() => {void drafts.load();}}>Retry loading setups</Button> : null}
    {(history ? history.more : setups.more) ? <Button variant="ghost" size="sm" disabled={setupsLoading} onClick={() => {if (history) void history.more?.(); else void drafts.load(true);}}>Load more setups</Button> : null}
    {page.loading && !page.items.length ? <output className="px-md text-label text-muted-foreground">Loading fleets…</output> : null}
    {!page.loading && !page.error && !page.items.length && !openSetups.length && !setupsLoading ? <p className="px-md text-label text-muted-foreground">Saved fleets will appear here.</p> : null}
    {page.error ? <output className="px-md py-sm text-label text-muted-foreground">{page.error} <Button variant="link" size="sm" disabled={page.loading} onClick={() => {void collection.load();}}>Refresh</Button></output> : null}
    {page.more ? <Button variant="ghost" size="sm" disabled={page.loading} onClick={() => {void collection.load(true);}}>Load more fleets</Button> : null}
    {selected ? <FleetLifecycleDialog action="delete" fleet={selected} workspace={workspace} onRemoved={removed} onClose={() => setSelected(null)} /> : null}
  </>;
}

function FleetRow({fleet, workspace, pathname, onNavigate, deleting, onDelete}: {
  fleet: NativeFleetSummary; workspace: string; pathname: string; onNavigate: () => void;
  deleting: boolean; onDelete: () => void;
}) {
  return <div className="group relative mb-xs">
    <NavItem asChild active={pathname === `/w/${workspace}/fleets/${fleet.id}`} className="gap-sm px-3 py-xs pr-12 hover:bg-muted hover:text-foreground data-[active=true]:bg-muted data-[active=true]:text-foreground" onClick={onNavigate}>
      <Link href={`/w/${workspace}/fleets/${fleet.id}`} prefetch={false} title={`${fleet.name} · ${fleet.status}`}>
        <span className={`size-1.5 shrink-0 rounded-full ${fleet.status === "active" ? "bg-pulse" : "bg-muted-foreground"}`} aria-hidden="true" />
        <span className="min-w-0 flex-1 truncate text-label text-foreground">{fleet.name}</span>
        {fleet.status === AGENTSFLEET_STATUS.ARCHIVED ? <span className="text-eyebrow text-muted-foreground">Archived</span> : null}
      </Link>
    </NavItem>
    <IconAction variant="ghost" label={`Permanently delete fleet: ${fleet.name}`} disabled={deleting}
      className="absolute right-xs top-1/2 -translate-y-1/2 text-muted-foreground opacity-0 transition-opacity duration-snap group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100"
      onClick={onDelete}><TrashIcon size={14} aria-hidden="true" /></IconAction>
  </div>;
}
