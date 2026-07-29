import { DashboardShellHeader } from "@agentsfleet/design-system";
import type { TenantWorkspace } from "@/lib/api/workspaces";
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
        className="app-glow-surface fixed inset-0 grid h-dvh grid-cols-1 grid-rows-[56px_1fr] md:grid-cols-[auto_1fr]"
        data-glow="dashboard"
      >
        <DashboardShellHeader>
          <ShellControls
            workspaces={workspaces}
            operatorScopes={operatorScopes}
            sidebarNavId={SIDEBAR_NAV_ID}
          />
          <div className="flex-1" />
          <WorkspaceSwitcher workspaces={workspaces} />
          <ThemeToggle />
          <ClientOnlyAuthUserButton />
        </DashboardShellHeader>

        <aside
          id={SIDEBAR_NAV_ID}
          className="hidden min-h-0 flex-col overflow-y-auto border-r border-border bg-muted py-4 md:flex"
        >
          <DesktopSidebarNavigation
            workspaces={workspaces}
            operatorScopes={operatorScopes}
          />
        </aside>

        <main className="app-dashboard-canvas min-h-0 overflow-y-auto px-4 py-6 sm:px-6 md:px-8 md:py-8 2xl:px-12 has-[#fleet-chat-transcript]:overflow-hidden has-[[data-page-layout]]:overflow-hidden">
          <div className="flex min-h-full w-full flex-col has-[#fleet-chat-transcript]:h-full has-[#fleet-chat-transcript]:min-h-0 has-[[data-page-layout]]:h-full has-[[data-page-layout]]:min-h-0">
            {children}
          </div>
        </main>
      </div>
    </WorkspaceCreationProvider>
  );
}
