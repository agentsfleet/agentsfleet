"use client";

import { usePathname } from "next/navigation";
import { cn, Nav } from "@agentsfleet/design-system";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import { workspaceIdFromPath } from "@/lib/workspace-routes";
import { SIDEBAR_COLUMN, useShellSidebarCollapsed } from "./shell-sidebar-state";
import { NavSection } from "./SidebarNavigationSections";
import { PRIMARY_NAV, resolveActiveHref, resolveHref } from "./sidebar-navigation-data";

import { Suspense } from "react";
import { DraftSidebarHistory } from "./DraftSidebarHistory";
import WorkspaceSwitcher from "./WorkspaceSwitcher";
import { RecentFleets } from "./RecentFleets";

export { resolveActiveHref } from "./sidebar-navigation-data";

type SidebarNavigationProps = {
  workspaces?: TenantWorkspace[];
  pathname: string;
  workspaceId: string | null;
  operatorScopes: string[];
  collapsed: boolean;
  onNavigate: () => void;
};

export function DesktopSidebarNavigation({
  workspaces,
  operatorScopes,
}: {
  workspaces: TenantWorkspace[];
  operatorScopes: string[];
}) {
  const pathname = usePathname();
  const collapsed = useShellSidebarCollapsed();
  const workspaceId =
    workspaceIdFromPath(pathname) ?? workspaces[0]?.id ?? null;
  return (
    <div
      className={cn(
        "h-full shrink-0 transition-all duration-snap ease-snap",
        collapsed ? SIDEBAR_COLUMN.aside.collapsed : SIDEBAR_COLUMN.aside.expanded,
      )}
    >
      <SidebarNavigation
        workspaces={workspaces}
        pathname={pathname}
        workspaceId={workspaceId}
        operatorScopes={operatorScopes}
        collapsed={collapsed}
        onNavigate={() => {}}
      />
    </div>
  );
}

export function SidebarNavigation({ pathname, workspaceId, operatorScopes, collapsed, onNavigate, workspaces = [] }: SidebarNavigationProps) {
  const activeHref = resolveActiveHref(PRIMARY_NAV.map((entry) => resolveHref(entry, workspaceId)), pathname);
  const shared = { activeHref, workspaceId, onNavigate, collapsed };
  return <Nav aria-label="Primary" className="flex h-full min-h-0 flex-col py-sm">
    <NavSection items={PRIMARY_NAV} {...shared} />
    {!collapsed ? <div className="min-h-0 flex-1 overflow-y-auto">
      <Suspense><DraftSidebarHistory workspace={workspaceId} onNavigate={onNavigate} /></Suspense>
      <RecentFleets workspaceId={workspaceId} pathname={pathname} onNavigate={onNavigate} />
    </div> : <div className="flex-1" />}
    <div className="mt-auto shrink-0 p-3">
      <WorkspaceSwitcher workspaces={workspaces} operatorScopes={operatorScopes} collapsed={collapsed} />
    </div>
  </Nav>;
}
