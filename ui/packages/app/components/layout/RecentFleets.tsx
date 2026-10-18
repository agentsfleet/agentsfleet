"use client";

import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import Link from "next/link";
import { NavItem, SectionLabel } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";
import { agentDisplayName } from "@/lib/fleets/agent-label";

type RecentFleet = { id: string; name: string };
type RecentRegistry = Record<string, RecentFleet[]>;
type Remember = (workspace: string, fleets: RecentFleet[], visited: boolean) => void;
const RECENT_LIMIT = 12;
const EMPTY_FLEETS: RecentFleet[] = [];
const RecentContext = createContext<RecentRegistry>({});
const RememberContext = createContext<Remember>(() => {});

// Session memory only: no private fleet names survive logout or cross accounts.
export function RecentFleetsProvider({ children }: { children: ReactNode }) {
  const [recent, setRecent] = useState<RecentRegistry>({});
  const remember = useCallback<Remember>((workspace, fleets, visited) => {
    setRecent((current) => {
      const previous = current[workspace] ?? EMPTY_FLEETS;
      const incoming = new Map(fleets.map((fleet) => [fleet.id, fleet]));
      const ordered = visited ? [...fleets, ...previous] : [...previous.map((fleet) => incoming.get(fleet.id) ?? fleet), ...fleets];
      const seen = new Set<string>();
      const next = ordered.filter((fleet) => !seen.has(fleet.id) && seen.add(fleet.id)).slice(0, RECENT_LIMIT);
      if (next.length === previous.length && next.every((fleet, index) => fleet.id === previous[index]?.id && fleet.name === previous[index].name)) return current;
      return { ...current, [workspace]: next };
    });
  }, []);
  return <RememberContext value={remember}><RecentContext value={recent}>{children}</RecentContext></RememberContext>;
}

export function useRememberFleets(workspace: string, fleets: RecentFleet[], visited = false) {
  const remember = useContext(RememberContext);
  useEffect(() => { remember(workspace, fleets, visited); }, [remember, workspace, fleets, visited]);
}

export function RecentFleetVisit({ workspace, id, name }: { workspace: string; id: string; name: string }) {
  const remember = useContext(RememberContext);
  useEffect(() => { remember(workspace, [{ id, name }], true); }, [remember, workspace, id, name]);
  return null;
}

export function RecentFleets({ workspaceId, pathname, onNavigate }: { workspaceId: string | null; pathname: string; onNavigate: () => void }) {
  const recent = useContext(RecentContext);
  const fleets = workspaceId ? recent[workspaceId] ?? EMPTY_FLEETS : EMPTY_FLEETS;
  return <div className="min-h-0 flex-1 overflow-y-auto px-3 pt-6">
    <SectionLabel className="px-md pb-md">Recents</SectionLabel>
    {workspaceId && fleets.length ? fleets.map((fleet) => {
      const href = workspacePath(workspaceId, `fleets/${fleet.id}`);
      return <NavItem key={fleet.id} asChild active={pathname === href || pathname.startsWith(`${href}/`)} onClick={onNavigate}>
        <Link href={href} prefetch={false} title={fleet.name} aria-label={agentDisplayName(fleet.id)}>
          <span className="min-w-0"><span className="block truncate">{agentDisplayName(fleet.id)}</span>
            <span className="block truncate text-label text-muted-foreground">{fleet.name}</span></span>
        </Link>
      </NavItem>;
    }) : <p className="px-md text-body-sm text-muted-foreground">Fleets you open appear here.</p>}
  </div>;
}
