"use client";

import { useState, useTransition } from "react";
import { usePathname, useRouter } from "next/navigation";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { DEFAULT_WORKSPACE_SUBPATH, workspaceIdFromPath, workspacePath, workspaceSubpath, workspaceSwitchSubpath } from "@/lib/workspace-routes";
import { useWorkspaceCreation } from "./WorkspaceCreationProvider";
import type { CreatedWorkspace } from "./useWorkspaceCreation";
import { switcherSections, WORKSPACE_LABEL } from "./workspace-groups";

export function useWorkspaceMenu(workspaces: TenantWorkspace[]) {
  const router = useRouter();
  const pathname = usePathname();
  const activeId = workspaceIdFromPath(pathname) ?? workspaces[0]?.id ?? null;
  const [pending, startTransition] = useTransition();
  const [createOpen, setCreateOpen] = useState(false);
  const creation = useWorkspaceCreation({ onSuccess: (workspace) => {
    setCreateOpen(false);
    startTransition(() => router.push(workspacePath(workspace.workspace_id, DEFAULT_WORKSPACE_SUBPATH)));
  } });
  const data = workspaceMenuData(workspaces, creation.createdWorkspaces, activeId);
  function setCreateDialogOpen(next: boolean) {
    if (next) creation.reset();
    else creation.dismiss();
    setCreateOpen(next);
  }
  function pick(id: string) {
    if (id === workspaceIdFromPath(pathname)) return;
    const label = data.menuWorkspaces.find((workspace) => workspace.id === id)?.name ?? WORKSPACE_LABEL.unnamed;
    captureProductEvent(EVENTS.workspace_switched, { workspace_id: id });
    startTransition(() => router.push(workspacePath(id, workspaceSwitchSubpath(workspaceSubpath(pathname)))));
    creation.showNotice("success", `Workspace changed to ${label}.`);
  }
  return { ...data, activeId, pending, createOpen, creation, setCreateDialogOpen, pick };
}

function workspaceMenuData(workspaces: TenantWorkspace[], created: readonly CreatedWorkspace[], activeId: string | null) {
  const known = [...workspaces, ...created];
  const routed = activeId !== null && !known.some((workspace) => workspace.id === activeId)
    ? { id: activeId, name: WORKSPACE_LABEL.current } : null;
  const sections = switcherSections(workspaces, created, routed);
  const menuWorkspaces = sections.flatMap((section) => section.workspaces);
  const active = menuWorkspaces.find((workspace) => workspace.id === activeId);
  const activeLabel = active ? active.name ?? WORKSPACE_LABEL.unnamed : WORKSPACE_LABEL.none;
  return { sections, menuWorkspaces, firstMenuId: menuWorkspaces[0]?.id ?? null, activeLabel };
}
