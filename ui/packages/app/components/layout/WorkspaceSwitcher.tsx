"use client";

import { useState, useTransition } from "react";
import { usePathname, useRouter } from "next/navigation";
import { ChevronDownIcon, PlusIcon } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  Toast,
  useResettableTimeout,
  type ToastSeverity,
} from "@agentsfleet/design-system";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import {
  workspaceIdFromPath,
  workspacePath,
  workspaceSubpath,
  workspaceSwitchSubpath,
} from "@/lib/workspace-routes";
import CreateWorkspaceDialogDynamic from "@/components/domain/island-dynamic/CreateWorkspaceDialogDynamic";

type Props = {
  workspaces: TenantWorkspace[];
  activeId: string | null;
};

const WORKSPACE_NOTICE_MS = 2800;

type Notice = {
  message: string;
  severity: ToastSeverity;
};

export default function WorkspaceSwitcher({
  workspaces,
  activeId,
}: Props) {
  const router = useRouter();
  const pathname = usePathname();
  const [pending, startTransition] = useTransition();
  const [createOpen, setCreateOpen] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const noticeTimer = useResettableTimeout();

  // Keep creation reachable when signup has not created a workspace yet.
  const active = workspaces.find((w) => w.id === activeId) ?? workspaces[0];
  const activeLabel = active?.name ?? active?.id ?? "No workspace";

  function workspaceLabel(id: string): string {
    const workspace = workspaces.find((w) => w.id === id);
    return workspace?.name ?? id;
  }

  function showNotice(severity: ToastSeverity, message: string) {
    setNotice({ severity, message });
    noticeTimer.start(() => setNotice(null), WORKSPACE_NOTICE_MS);
  }

  // Switching a workspace is a navigation: push `/w/{id}/{section}` so the user
  // stays in the same section of the new workspace. No cookie, no server action —
  // the URL is authoritative. A resource-detail path (`fleets/{id}`) collapses to
  // its section (`fleets`) since the target workspace won't own that resource;
  // from a tenant page (no `/w/` segment) the sub-path is empty → the home.
  function pick(id: string) {
    // No-op only when we're already ON this workspace's route — `activeId` is a
    // display fallback (the first workspace) on tenant pages, so comparing to it
    // would wrongly block navigating into the default from e.g. /settings/billing.
    if (id === workspaceIdFromPath(pathname)) return;
    const label = workspaceLabel(id);
    captureProductEvent(EVENTS.workspace_switched, { workspace_id: id });
    startTransition(() => {
      router.push(workspacePath(id, workspaceSwitchSubpath(workspaceSubpath(pathname))));
    });
    showNotice("success", `Workspace changed to ${label}.`);
  }

  return (
    <>
      <div className="inline-flex flex-wrap items-center gap-2">
        <DropdownMenu>
          <DropdownMenuTrigger
            className="inline-flex items-center gap-2 rounded-md border border-border-strong bg-card px-lg py-md font-mono text-eyebrow text-foreground transition-colors duration-snap ease-snap enabled:hover:bg-secondary disabled:cursor-wait disabled:opacity-60"
            aria-label="Select workspace"
            data-testid="workspace-switcher"
            disabled={pending}
          >
            <span className="max-w-trim overflow-hidden text-ellipsis whitespace-nowrap">
              {activeLabel}
            </span>
            <ChevronDownIcon size={14} aria-hidden="true" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="max-h-96 overflow-y-auto">
            <DropdownMenuLabel>Workspace</DropdownMenuLabel>
            <DropdownMenuSeparator />
            {workspaces.map((ws) => (
              <DropdownMenuItem
                key={ws.id}
                onSelect={() => pick(ws.id)}
                data-active={ws.id === active?.id ? "true" : undefined}
              >
                <span className="flex-1">{ws.name ?? ws.id}</span>
                {ws.id === active?.id ? <span aria-hidden="true">✓</span> : null}
              </DropdownMenuItem>
            ))}
            {workspaces.length > 0 ? <DropdownMenuSeparator /> : null}
            <DropdownMenuItem onSelect={() => setCreateOpen(true)} data-testid="workspace-new">
              <PlusIcon size={14} aria-hidden="true" />
              <span className="flex-1">Create workspace</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <CreateWorkspaceDialogDynamic
        open={createOpen}
        onOpenChange={setCreateOpen}
        onCreated={(name) => showNotice("success", `Workspace created: ${name}.`)}
      />
      <div className="pointer-events-none fixed right-4 top-16 z-50 max-w-sm">
        <Toast
          visible={notice !== null}
          severity={notice?.severity ?? "info"}
          data-testid="workspace-toast"
        >
          {notice?.message ?? ""}
        </Toast>
      </div>
    </>
  );
}
