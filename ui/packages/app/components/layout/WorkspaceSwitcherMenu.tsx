"use client";

import { useRef, useState, useTransition } from "react";
import { usePathname, useRouter } from "next/navigation";
import { FolderIcon, PlusIcon } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
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
import { useWorkspaceCreation } from "./WorkspaceCreationProvider";
import { WorkspaceSwitcherTrigger } from "./WorkspaceSwitcherTrigger";

type WorkspaceSwitcherMenuProps = {
  open: boolean;
  workspaces: TenantWorkspace[];
  onOpenChange: (open: boolean) => void;
};

export default function WorkspaceSwitcherMenu({
  open,
  workspaces,
  onOpenChange,
}: WorkspaceSwitcherMenuProps) {
  const router = useRouter();
  const pathname = usePathname();
  const activeId =
    workspaceIdFromPath(pathname) ?? workspaces[0]?.id ?? null;
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

  function pick(id: string) {
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
        <DropdownMenu open={open} onOpenChange={onOpenChange}>
          <DropdownMenuTrigger asChild>
            <WorkspaceSwitcherTrigger
              ref={switcherTriggerRef}
              activeLabel={activeLabel}
              busy={pending}
              aria-label="Select workspace"
              data-testid="workspace-switcher"
              disabled={pending}
            />
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
        restoreFocus={() => switcherTriggerRef.current?.focus()}
      />
    </>
  );
}
