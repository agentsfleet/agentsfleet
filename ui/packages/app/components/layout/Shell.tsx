"use client";

import { useEffect, useState } from "react";
import { usePathname } from "next/navigation";
import Link from "next/link";
import { PanelLeftCloseIcon, PanelLeftOpenIcon } from "lucide-react";
import { Button, cn, DashboardShellHeader, WakePulse } from "@agentsfleet/design-system";
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
      // A fixed application frame, not a growing document: the header and the
      // navigation rail stay put and the content region below owns the scroll.
      // A page that wants the viewport — the fleet console's chat — then needs
      // only an ordinary full-height child, with no height literal of its own.
      className={cn(
        "app-glow-surface grid h-dvh grid-rows-[56px_1fr]",
        "fixed inset-0",
        collapsed ? "md:grid-cols-[64px_1fr]" : "md:grid-cols-[240px_1fr]",
      )}
      data-glow="dashboard"
    >
      <DashboardShellHeader>
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
      </DashboardShellHeader>

      <aside
        id={SIDEBAR_NAV_ID}
        className="hidden md:flex min-h-0 flex-col overflow-y-auto border-r border-border bg-muted py-4"
      >
        <SidebarNavigation
          pathname={pathname}
          workspaceId={linkWorkspaceId}
          operatorScopes={operatorScopes}
          collapsed={collapsed}
          onNavigate={() => {}}
        />
      </aside>

      <main className="app-dashboard-canvas min-h-0 overflow-y-auto px-4 py-6 sm:px-6 md:px-8 md:py-8 2xl:px-12 has-[#fleet-chat-transcript]:overflow-hidden has-[[data-page-layout]]:overflow-hidden">
        {/* `min-h-full` + column flow: ordinary pages grow past the viewport
            and scroll here, while a bounded workspace claims the region and
            scrolls inside itself instead. */}
        <div className="flex min-h-full w-full flex-col has-[#fleet-chat-transcript]:h-full has-[#fleet-chat-transcript]:min-h-0 has-[[data-page-layout]]:h-full has-[[data-page-layout]]:min-h-0">{children}</div>
      </main>
    </div>
  );
}
