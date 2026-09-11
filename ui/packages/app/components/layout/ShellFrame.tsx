import { DashboardShellHeader } from "@agentsfleet/design-system";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import CanvasScrollbarProbe from "./CanvasScrollbarProbe";
import ClientOnlyAuthUserButton from "./ClientOnlyAuthUserButton";
import {
  DesktopSidebarNavigation,
} from "./SidebarNavigation";
import { ShellControls } from "./ShellControls";
import ThemeToggle from "./ThemeToggle";
import { WorkspaceCreationProvider } from "./WorkspaceCreationProvider";
import WorkspaceSwitcher from "./WorkspaceSwitcher";

const SIDEBAR_NAV_ID = "app-sidebar-nav";

type ShellFrameProps = {
  children: React.ReactNode;
  workspaces?: TenantWorkspace[];
  operatorScopes?: string[];
};

export function ShellFrame({
  children,
  workspaces = [],
  operatorScopes = [],
}: ShellFrameProps) {
  const knownWorkspaceIds = workspaces.map((workspace) => workspace.id);
  return (
    <WorkspaceCreationProvider knownWorkspaceIds={knownWorkspaceIds}>
      <div
        className="app-surface fixed inset-0 grid h-dvh grid-cols-1 grid-rows-[56px_1fr] md:grid-cols-[auto_1fr]"
        data-surface="dashboard"
      >
        {/*
          The header owns no horizontal padding of its own: its leading cluster
          has to start where the sidebar column starts, and each cluster pads
          itself instead.
        */}
        <DashboardShellHeader className="px-0 md:px-0">
          <ShellControls
            workspaces={workspaces}
            operatorScopes={operatorScopes}
            sidebarNavId={SIDEBAR_NAV_ID}
          />
          <div className="app-shell-trailing ml-auto flex min-w-0 items-center gap-md md:gap-xl">
            <WorkspaceSwitcher workspaces={workspaces} />
            <ThemeToggle />
            <div className="flex shrink-0 items-center"><ClientOnlyAuthUserButton /></div>
          </div>
        </DashboardShellHeader>

        <aside
          id={SIDEBAR_NAV_ID}
          className="hidden min-h-0 flex-col overflow-y-auto border-r border-border bg-card py-4 md:flex"
        >
          <DesktopSidebarNavigation
            workspaces={workspaces}
            operatorScopes={operatorScopes}
          />
        </aside>

        <CanvasScrollbarProbe />

        <main className="app-dashboard-canvas min-h-0 overflow-y-auto py-6 md:py-8 has-[#fleet-chat-transcript]:overflow-hidden has-[[data-page-layout]]:overflow-hidden">
          <div className="flex min-h-full w-full flex-col has-[#fleet-chat-transcript]:h-full has-[#fleet-chat-transcript]:min-h-0 has-[[data-page-layout]]:h-full has-[[data-page-layout]]:min-h-0">
            {children}
          </div>
        </main>
      </div>
    </WorkspaceCreationProvider>
  );
}
