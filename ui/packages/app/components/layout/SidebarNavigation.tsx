"use client";

import { usePathname } from "next/navigation";
import { cn, Nav, TooltipProvider } from "@agentsfleet/design-system";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import { workspaceIdFromPath } from "@/lib/workspace-routes";
import { SIDEBAR_COLUMN, useShellSidebarCollapsed } from "./shell-sidebar-state";
import { NavSection } from "./SidebarNavigationSections";
import { PRIMARY_NAV, resolveActiveHref, resolveHref } from "./sidebar-navigation-data";

import { Suspense, useEffect } from "react";
import { setAnalyticsContext } from "@/lib/analytics/posthog";
import { DraftSidebarHistory } from "./DraftSidebarHistory";
import WorkspaceSwitcher from "./WorkspaceSwitcher";
import type { TenantBilling } from "@/lib/types";
import { BalanceLink } from "./BalanceLink";
import { ShellControls, SidebarExpandControl } from "./ShellControls";

export { resolveActiveHref } from "./sidebar-navigation-data";

type SidebarNavigationProps = {
  workspaces?: TenantWorkspace[];
  billing?: TenantBilling | null;
  pathname: string;
  workspaceId: string | null;

  collapsed: boolean;
  onNavigate: () => void;
};

export function DesktopSidebarNavigation({
  workspaces,
  billing,
}: {
  workspaces: TenantWorkspace[];
  billing?: TenantBilling | null;

}) {
  const pathname = usePathname();
  const collapsed = useShellSidebarCollapsed();
  const workspaceId =
    workspaceIdFromPath(pathname) ?? workspaces[0]?.id ?? null;
  useEffect(() => {
    setAnalyticsContext({workspaceId, workspaceCount: workspaces.length});
  }, [workspaceId, workspaces.length]);
  return (
    <div
      className={cn(
        "h-full shrink-0 transition-all duration-snap ease-snap",
        collapsed ? SIDEBAR_COLUMN.aside.collapsed : SIDEBAR_COLUMN.aside.expanded,
      )}
    >
      <SidebarNavigation
        workspaces={workspaces}
        billing={billing}
        pathname={pathname}
        workspaceId={workspaceId}
        collapsed={collapsed}
        onNavigate={() => {}}
      />
    </div>
  );
}

export function SidebarNavigation({ pathname, workspaceId, collapsed, onNavigate, workspaces = [], billing }: SidebarNavigationProps) {
  const activeHref = resolveActiveHref(PRIMARY_NAV.map((entry) => resolveHref(entry, workspaceId)), pathname);
  const shared = { activeHref, workspaceId, onNavigate, collapsed };
  return <TooltipProvider><Nav aria-label="Primary" className="flex h-full min-h-0 flex-col gap-lg py-sm">
    <ShellControls workspaceId={workspaceId} collapsed={collapsed} />
    <div className="min-h-0 shrink overflow-y-auto py-sm"><NavSection items={PRIMARY_NAV} {...shared} /></div>
    {!collapsed ? <div className="hidden min-h-0 flex-1 overflow-y-auto md:block">
      <Suspense><DraftSidebarHistory account={workspaces.find(item => item.id === workspaceId)?.account.tenant_id ?? null} workspace={workspaceId} onNavigate={onNavigate} /></Suspense>
    </div> : <div className="flex-1" />}
    <div className="mt-auto flex shrink-0 flex-col gap-md px-sm pb-sm md:px-lg">
      {!collapsed && billing ? <div className="hidden px-lg md:block"><BalanceLink balanceNanos={billing.balance_nanos} isExhausted={billing.is_exhausted} /></div> : null}
      <SidebarExpandControl collapsed={collapsed} />
      <WorkspaceSwitcher workspaces={workspaces} collapsed={collapsed} billing={billing} />
    </div>
  </Nav></TooltipProvider>;
}
