"use client";

import { useRef, useState, useTransition } from "react";
import { usePathname, useRouter } from "next/navigation";
import { ChevronDownIcon, FolderIcon, PlusIcon } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from "@agentsfleet/design-system";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import {
  DEFAULT_WORKSPACE_SUBPATH,
  workspaceIdFromPath,
  workspacePath,
  workspaceSubpath,
  workspaceSwitchSubpath,
} from "@/lib/workspace-routes";
import CreateWorkspaceDialogDynamic from "@/components/domain/island-dynamic/CreateWorkspaceDialogDynamic";
import { useWorkspaceCreation } from "@/components/layout/WorkspaceCreationProvider";

type Props = {
  workspaces: TenantWorkspace[];
  activeId: string | null;
};

export default function WorkspaceSwitcher({ workspaces, activeId }: Props) {
  const router = useRouter();
  const pathname = usePathname();
  const [pending, startTransition] = useTransition();
  const [createOpen, setCreateOpen] = useState(false);
  const switcherTriggerRef = useRef<HTMLButtonElement>(null);

  const creation = useWorkspaceCreation({
    onSuccess: (workspace) => {
      setCreateOpen(false);
      startTransition(() => {
        router.push(
          workspacePath(workspace.workspace_id, DEFAULT_WORKSPACE_SUBPATH),
        );
      });
    },
  });
  const visibleWorkspaces = [
    ...workspaces,
    ...creation.createdWorkspaces.filter(
      (created) => !workspaces.some((workspace) => workspace.id === created.id),
    ),
  ];
  const routedWorkspace =
    activeId !== null &&
    !visibleWorkspaces.some((workspace) => workspace.id === activeId)
      ? { id: activeId, name: "Current workspace" }
      : null;
  const menuWorkspaces = routedWorkspace
    ? [routedWorkspace, ...visibleWorkspaces]
    : visibleWorkspaces;

  // Keep creation reachable when signup has not created a workspace yet.
  const active =
    activeId === null
      ? visibleWorkspaces[0]
      : menuWorkspaces.find((workspace) => workspace.id === activeId);
  const activeLabel = active
    ? (active.name ?? "Unnamed workspace")
    : "No workspace";

  function workspaceLabel(id: string): string {
    const workspace = visibleWorkspaces.find(
      (candidate) => candidate.id === id,
    );
    return workspace?.name ?? "Unnamed workspace";
  }

  function setCreateDialogOpen(nextOpen: boolean) {
    if (nextOpen) {
      creation.reset();
      setCreateOpen(true);
      return;
    }

    setCreateOpen(false);
    creation.dismiss();
  }

  function restoreCreateFocus() {
    switcherTriggerRef.current?.focus();
  }

  // Switching a workspace is a navigation: push `/w/{id}/{section}` so the user
  // stays in the same section of the new workspace. No cookie, no server action —
  // the URL is authoritative. A resource-detail path (`fleets/{id}`) collapses to
  // its section (`fleets`) since the target workspace won't own that resource;
  // from a tenant page (no `/w/` segment), the switch lands on the fleet wall.
  function pick(id: string) {
    // No-op only when we're already ON this workspace's route — `activeId` is a
    // display fallback (the first workspace) on tenant pages, so comparing to it
    // would wrongly block navigating into the default from e.g. /settings/billing.
    if (id === workspaceIdFromPath(pathname)) return;
    const label = workspaceLabel(id);
    captureProductEvent(EVENTS.workspace_switched, { workspace_id: id });
    startTransition(() => {
      router.push(
        workspacePath(id, workspaceSwitchSubpath(workspaceSubpath(pathname))),
      );
    });
    creation.showNotice("success", `Workspace changed to ${label}.`);
  }

  return (
    <>
      <div className="inline-flex flex-wrap items-center gap-2">
        <DropdownMenu>
          <DropdownMenuTrigger
            ref={switcherTriggerRef}
            className="inline-flex items-center gap-2 rounded-md border border-border-strong bg-card px-lg py-md font-mono text-eyebrow text-foreground transition-colors duration-snap ease-snap enabled:hover:bg-secondary disabled:cursor-wait disabled:opacity-60"
            aria-label="Select workspace"
            data-testid="workspace-switcher"
            disabled={pending}
          >
            <FolderIcon
              size={14}
              strokeWidth={1.75}
              aria-hidden="true"
              className="text-muted-foreground"
            />
            <span className="max-w-trim overflow-hidden text-ellipsis whitespace-nowrap">
              {activeLabel}
            </span>
            <ChevronDownIcon size={14} aria-hidden="true" />
          </DropdownMenuTrigger>
          <DropdownMenuContent
            align="start"
            className="max-w-trim overflow-hidden"
          >
            <DropdownMenuLabel>Workspace</DropdownMenuLabel>
            <DropdownMenuSeparator />
            <div
              className="max-h-80 overflow-y-auto"
              data-testid="workspace-list-scroll"
            >
              {menuWorkspaces.map((workspace) => {
                const label = workspace.name ?? "Unnamed workspace";
                return (
                  <DropdownMenuItem
                    key={workspace.id}
                    onSelect={() => pick(workspace.id)}
                    data-active={workspace.id === activeId ? "true" : undefined}
                  >
                    <FolderIcon
                      size={14}
                      strokeWidth={1.75}
                      aria-hidden="true"
                      className="text-muted-foreground"
                    />
                    <span className="min-w-0 flex-1 truncate" title={label}>
                      {label}
                    </span>
                    {workspace.id === activeId ? (
                      <span aria-hidden="true">✓</span>
                    ) : null}
                  </DropdownMenuItem>
                );
              })}
            </div>
            {menuWorkspaces.length > 0 ? <DropdownMenuSeparator /> : null}
            <DropdownMenuItem
              onSelect={() => setCreateDialogOpen(true)}
              disabled={creation.locked}
              aria-disabled={creation.locked || undefined}
              data-testid="workspace-new"
            >
              <PlusIcon size={14} aria-hidden="true" />
              <span className="flex-1">Create workspace</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <CreateWorkspaceDialogDynamic
        open={createOpen}
        pending={creation.pending}
        error={creation.error}
        onOpenChange={setCreateDialogOpen}
        onSubmit={creation.create}
        restoreFocus={restoreCreateFocus}
      />
    </>
  );
}
