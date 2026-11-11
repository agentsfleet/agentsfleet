import type { WaitingInvite } from "@/lib/api/invites";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import type { TenantBilling } from "@/lib/types";
import { RouteLoading } from "./RouteLoading";
import { InviteNotice } from "./InviteNotice";
import {
  DesktopSidebarNavigation,
} from "./SidebarNavigation";
import { WorkspaceCreationProvider } from "./WorkspaceCreationProvider";
import { DraftSidebarProvider } from "./draft-sidebar-state";
import { FleetNoticeProvider } from "./FleetNoticeProvider";

const SIDEBAR_NAV_ID = "app-sidebar-nav";

type ShellFrameProps = {
  children: React.ReactNode;
  workspaces?: TenantWorkspace[];

  /** Null when the read failed or there is no session: the header says nothing. */
  billing?: TenantBilling | null;
  /** Invites waiting for the signed-in person; empty hides the notice. */
  waitingInvites?: readonly WaitingInvite[];
};

export function ShellFrame({
  children,
  workspaces = [],

  billing = null,
  waitingInvites = [],
}: ShellFrameProps) {
  const knownWorkspaceIds = workspaces.map((workspace) => workspace.id);
  return (
    <WorkspaceCreationProvider knownWorkspaceIds={knownWorkspaceIds}>
      <DraftSidebarProvider><FleetNoticeProvider><div
        className="app-surface app-shell fixed inset-0 grid h-dvh grid-cols-[auto_minmax(0,1fr)] bg-surface-deep py-md pr-md"
        data-surface="dashboard"
      >
        <aside
          id={SIDEBAR_NAV_ID}
          className="app-sidebar flex min-h-0 flex-col"
        >
          <DesktopSidebarNavigation
            workspaces={workspaces}
            billing={billing}
          />
        </aside>

        <main className="app-dashboard-canvas min-h-0 min-w-0 overflow-y-auto rounded-lg border border-border-subtle bg-background has-[#fleet-chat-transcript]:[--app-canvas-block:0px] has-[#fleet-chat-transcript]:overflow-hidden md:has-[[data-page-layout]]:overflow-hidden">
          <div className="flex min-h-full w-full flex-col has-[#fleet-chat-transcript]:h-full has-[#fleet-chat-transcript]:min-h-0 md:has-[[data-page-layout]]:h-full md:has-[[data-page-layout]]:min-h-0">
            <InviteNotice waiting={waitingInvites} />
            {children}
            {/* Next can briefly remove the outgoing route before its loader
                mounts. CSS fills that empty slot in the same paint. */}
            <div data-testid="shell-route-loading" className="hidden only:flex flex-1">
              <RouteLoading />
            </div>
          </div>
        </main>
      </div></FleetNoticeProvider></DraftSidebarProvider>
    </WorkspaceCreationProvider>
  );
}
