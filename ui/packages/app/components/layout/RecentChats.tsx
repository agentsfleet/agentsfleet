"use client";

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { usePathname, useRouter } from "next/navigation";
import type { z } from "zod";
import Link from "next/link";
import { NavItem, IconAction, Button } from "@agentsfleet/design-system";
import { Trash2Icon } from "lucide-react";
import { FleetListSchema, type NativeFleetSummary } from "@/lib/api/fleet-detail";
import { DraftCardsSchema } from "@/lib/api/draft-types";
import { sidebarDrafts } from "@/lib/api/draft-summary";
import { RecentCollection } from "@/lib/api/recent-collection";
import { DraftChatList } from "./DraftChatList";
import { FleetDeleteDialog } from "./FleetDeleteDialog";

function useRecentPage<T extends {id: string}>(path: string, schema: z.ZodType<{items: T[]; next_cursor: string | null}>) {
  const [collection, setCollection] = useState<RecentCollection<T> | null>(null);
  useEffect(() => {
    const current = new RecentCollection(path, schema);
    setCollection(current); void current.load();
    return () => current.dispose();
  }, [path, schema]);
  return collection;
}

export function RecentFleetList({workspace, onNavigate}: {workspace: string; onNavigate: () => void}) {
  const collection = useRecentPage(`/live/v1/workspaces/${encodeURIComponent(workspace)}/fleets`, FleetListSchema);
  return collection ? <FleetItems collection={collection} workspace={workspace} onNavigate={onNavigate} /> : null;
}

function FleetItems({collection, workspace, onNavigate}: {collection: RecentCollection<NativeFleetSummary>; workspace: string; onNavigate: () => void}) {
  const page = useSyncExternalStore(collection.subscribe, collection.getSnapshot, collection.getSnapshot);
  const pathname = usePathname();
  const router = useRouter();
  const [selected, setSelected] = useState<NativeFleetSummary | null>(null);
  const previousPath = useRef(pathname);
  useEffect(() => {if (previousPath.current !== pathname) {previousPath.current = pathname; void collection.load();}}, [collection, pathname]);
  const removed = (id: string): void => {
    collection.remove(id); setSelected(null);
    if (pathname.includes(`/fleets/${id}`)) router.replace(`/w/${workspace}/fleets/new`);
    router.refresh();
  };
  return <>
    {page.items.map(fleet => <div key={fleet.id} className="group relative mb-xs">
      <NavItem asChild active={pathname === `/w/${workspace}/fleets/${fleet.id}`} className="py-xs pr-12" onClick={onNavigate}>
        <Link href={`/w/${workspace}/fleets/${fleet.id}`} prefetch={false} title={`${fleet.name} · ${fleet.status}`}>
          <span className={`min-w-0 flex-1 truncate ${fleet.status === "active" ? "text-pulse" : "text-foreground"}`}>{fleet.name}</span>
        </Link>
      </NavItem>
      <IconAction variant="ghost" label={`Delete fleet: ${fleet.name}`} disabled={selected !== null}
        className="absolute right-xs top-1/2 -translate-y-1/2 text-muted-foreground opacity-0 transition-opacity duration-snap group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100"
        onClick={() => setSelected(fleet)}><Trash2Icon size={14} aria-hidden="true" /></IconAction>
    </div>)}
    {page.error ? <output className="px-md py-sm text-label text-muted-foreground">{page.error} <Button variant="link" size="sm" disabled={page.loading} onClick={() => {void collection.load();}}>Refresh</Button></output> : null}
    {page.more ? <Button variant="ghost" size="sm" disabled={page.loading} onClick={() => {void collection.load(true);}}>Load more fleets</Button> : null}
    {selected ? <FleetDeleteDialog fleet={selected} workspace={workspace} onRemoved={removed} onClose={() => setSelected(null)} /> : null}
  </>;
}

export function StandaloneDraftHistory({workspace, subject, onNavigate}: {workspace: string; subject: string; onNavigate: () => void}) {
  const collection = useRecentPage(`/live/v1/workspaces/${encodeURIComponent(workspace)}/drafts`, DraftCardsSchema);
  return collection ? <DraftItems collection={collection} workspace={workspace} subject={subject} onNavigate={onNavigate} /> : null;
}

function DraftItems({collection, workspace, subject, onNavigate}: {collection: RecentCollection<z.infer<typeof DraftCardsSchema>["items"][number]>; workspace: string; subject: string; onNavigate: () => void}) {
  const page = useSyncExternalStore(collection.subscribe, collection.getSnapshot, collection.getSnapshot);
  const cards = sidebarDrafts(page.items, null).filter(card => !card.linkedFleet);
  const history = {workspace, subject, cards, fresh: null, remove: null, owner: null, locked: page.loading, current: null,
    more: null, status: page.error ? "unavailable" as const : "ready" as const};
  return <>
    <DraftChatList history={history} cards={cards} onNavigate={onNavigate} onRemoved={id => collection.remove(id)} />
    {page.error ? <output className="px-md py-sm text-label text-muted-foreground">{page.error}</output> : null}
    {page.more ? <Button variant="ghost" size="sm" disabled={page.loading} onClick={() => {void collection.load(true);}}>Load more chats</Button> : null}
  </>;
}
