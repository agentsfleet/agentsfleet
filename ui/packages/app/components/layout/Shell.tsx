"use client";

import { useEffect, useState } from "react";
import { usePathname } from "next/navigation";
import Link from "next/link";
import { PanelLeftCloseIcon, PanelLeftOpenIcon } from "lucide-react";
import { Button, cn, WakePulse } from "@agentsfleet/design-system";
import { setAnalyticsContext } from "@/lib/analytics/posthog";
import {
  DEFAULT_WORKSPACE_SUBPATH,
  workspaceIdFromPath,
  workspacePath,
} from "@/lib/workspace-routes";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import { MobileNavigation, SidebarNavigation } from "./SidebarNavigation";
import WorkspaceSwitcher from "./WorkspaceSwitcher";
import ThemeToggle from "./ThemeToggle";
import ClientOnlyAuthUserButton from "./ClientOnlyAuthUserButton";

const SIDEBAR_NAV_ID = "app-sidebar-nav";

type ShellProps = {
  children: React.ReactNode;
  workspaces?: TenantWorkspace[];
  operatorScopes?: string[];
};

export default function Shell({
  children,
  workspaces = [],
  operatorScopes = [],
}: ShellProps) {
  const pathname = usePathname();
  const activeWorkspaceId = workspaceIdFromPath(pathname);
  const linkWorkspaceId = activeWorkspaceId ?? workspaces[0]?.id ?? null;
  const [collapsed, setCollapsed] = useState(false);

  useEffect(() => {
    setAnalyticsContext({
      workspaceId: activeWorkspaceId,
      workspaceCount: workspaces.length,
    });
  }, [activeWorkspaceId, workspaces.length]);

  return (
    <div
      className={cn(
        "app-glow-surface grid min-h-screen grid-rows-[56px_1fr]",
        collapsed ? "md:grid-cols-[64px_1fr]" : "md:grid-cols-[240px_1fr]",
      )}
      data-glow="dashboard"
    >
      <header className="col-span-full sticky top-0 z-40 flex items-center gap-4 px-4 md:px-6 border-b border-border bg-background/85 backdrop-blur">
        <MobileNavigation
          pathname={pathname}
          workspaceId={linkWorkspaceId}
          operatorScopes={operatorScopes}
        />

        <Button
          type="button"
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          aria-expanded={!collapsed}
          aria-controls={SIDEBAR_NAV_ID}
          variant="ghost"
          size="icon"
          className="hidden md:inline-flex -ml-2"
          onClick={() => setCollapsed((current) => !current)}
        >
          {collapsed ? <PanelLeftOpenIcon size={18} /> : <PanelLeftCloseIcon size={18} />}
        </Button>

        <Link
          href={linkWorkspaceId ? workspacePath(linkWorkspaceId, DEFAULT_WORKSPACE_SUBPATH) : "/"}
          className="inline-flex items-center gap-2 font-mono text-sm font-medium tracking-tight text-foreground no-underline"
          aria-label="agentsfleet home"
        >
          <WakePulse live className="inline-block w-3 h-3 rounded-full bg-pulse" aria-hidden="true" />
          <span>agentsfleet</span>
        </Link>

        <div className="flex-1" />
        <WorkspaceSwitcher workspaces={workspaces} activeId={linkWorkspaceId} />
        <ThemeToggle />
        <ClientOnlyAuthUserButton />
      </header>

      <aside
        id={SIDEBAR_NAV_ID}
        className="hidden md:flex flex-col bg-muted border-r border-border sticky top-14 h-[calc(100vh-56px)] overflow-y-auto py-4"
      >
        <SidebarNavigation
          pathname={pathname}
          workspaceId={linkWorkspaceId}
          operatorScopes={operatorScopes}
          collapsed={collapsed}
          onNavigate={() => {}}
        />
      </aside>

      <main className="app-dashboard-canvas overflow-auto px-4 py-6 sm:px-6 md:px-8 md:py-8 2xl:px-12">
        <div className="w-full">{children}</div>
      </main>
    </div>
  );
}
