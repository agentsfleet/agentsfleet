"use client";

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { usePathname, useRouter } from "next/navigation";
import Link from "next/link";
import { NavItem, IconAction, Button, Spinner } from "@agentsfleet/design-system";
import { TrashIcon } from "@phosphor-icons/react/ssr";
import { useRecentPage } from "@/lib/api/use-recent-page";
import { FleetListSchema, type NativeFleetSummary } from "@/lib/api/fleet-detail";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import type { RecentCollection } from "@/lib/api/recent-collection";
import { FleetLifecycleDialog } from "./FleetLifecycleDialog";

// Reuse a recent list during rapid switches and reconcile server-side changes on later navigation.
const FLEET_LIST_FRESH_MS = 30_000;

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
  useEffect(() => {
    const previous = previousPath.current;
    if (previous === pathname) return;
    previousPath.current = pathname;
    const fleetPath = `/w/${workspace}/fleets/`;
    const {items} = collection.getSnapshot();
    // A first message lands on a new fleet's page. Moving between loaded
    // fleets can keep the existing pages and update only the selection.
    const known = (path: string) => items.some(fleet => path === `${fleetPath}${fleet.id}`);
    void collection.refresh(known(previous) && known(pathname) ? FLEET_LIST_FRESH_MS : 0);
  }, [collection, pathname, workspace]);
  const removed = (id: string): void => {
    collection.remove(id); setSelected(null);
    if (pathname.includes(`/fleets/${id}`)) router.replace(`/w/${workspace}/fleets/new`);
    else router.refresh();
  };
  return <>
    {page.items.map(fleet => <FleetRow key={fleet.id} fleet={fleet} workspace={workspace} pathname={pathname} onNavigate={onNavigate}
      deleting={selected !== null} onDelete={() => setSelected(fleet)} />)}
    {page.loading && !page.items.length ? <Spinner size="sm" label="Loading fleets…" className="px-lg py-md text-eyebrow leading-eyebrow" /> : null}
    {!page.loading && !page.error && !page.items.length ? <p className="px-lg py-md text-eyebrow leading-eyebrow text-muted-foreground">Your fleets will appear here.</p> : null}
    {page.more ? <Button variant="ghost" size="sm" disabled={page.loading} onClick={() => {void collection.load(true);}}>Load more fleets</Button> : null}
    {selected ? <FleetLifecycleDialog action="delete" fleet={selected} workspace={workspace} onRemoved={removed} onClose={() => setSelected(null)} /> : null}
  </>;
}

function FleetRow({fleet, workspace, pathname, onNavigate, deleting, onDelete}: {
  fleet: NativeFleetSummary; workspace: string; pathname: string; onNavigate: () => void; deleting: boolean; onDelete: () => void;
}) {
  return <div className="group flex min-w-0 items-center">
    <NavItem asChild appearance="list" active={pathname === `/w/${workspace}/fleets/${fleet.id}`} className="min-w-0 flex-1 px-lg" onClick={onNavigate}>
      <Link href={`/w/${workspace}/fleets/${fleet.id}`} prefetch={false} title={`${fleet.name} · ${fleet.status}`}>
        <span className="min-w-0 flex-1 truncate font-semibold text-foreground">{fleet.name}</span>
        {fleet.status === AGENTSFLEET_STATUS.ARCHIVED ? <span className="shrink-0 text-eyebrow text-text-dim">Archived</span> : null}
      </Link>
    </NavItem>
    <IconAction revealOnRowFocus variant="ghost" label={`Permanently delete fleet: ${fleet.name}`} disabled={deleting}
      className="shrink-0"
      onClick={onDelete}><TrashIcon size={16} aria-hidden="true" /></IconAction>
  </div>;
}
