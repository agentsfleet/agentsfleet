"use client";

import { type RefObject, useEffect, useId, useRef, useState, useTransition } from "react";
import { usePathname, useRouter } from "next/navigation";
import { FolderIcon, PlusIcon } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Nav,
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
import { type SwitcherSection, switcherSections, WORKSPACE_LABEL } from "./workspace-groups";
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
  const [portalContainer, setPortalContainer] = useState<HTMLElement | null>(null);
  const switcherTriggerRef = useRef<HTMLButtonElement>(null);
  const firstItemRef = useRef<HTMLDivElement>(null);
  const entryFocusedRef = useRef(false);

  useEffect(() => {
    if (!open) entryFocusedRef.current = false;
  }, [open]);

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
      ? { id: activeId, name: WORKSPACE_LABEL.current }
      : null;
  // Sections by account; one unlabelled section while the caller holds only
  // their own.
  const sections = switcherSections(
    workspaces,
    creation.createdWorkspaces,
    routedWorkspace,
  );
  const menuWorkspaces = sections.flatMap((section) => section.workspaces);
  const firstMenuId = menuWorkspaces[0]?.id ?? null;
  const active =
    activeId === null
      ? visibleWorkspaces[0]
      : menuWorkspaces.find((workspace) => workspace.id === activeId);
  const activeLabel = active
    ? (active.name ?? WORKSPACE_LABEL.unnamed)
    : WORKSPACE_LABEL.none;

  function workspaceLabel(id: string): string {
    const workspace = visibleWorkspaces.find(
      (candidate) => candidate.id === id,
    );
    return workspace?.name ?? WORKSPACE_LABEL.unnamed;
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
      <Nav ref={setPortalContainer} aria-label="Workspaces" className="inline-flex min-w-0 items-center gap-2">
        <DropdownMenu open={open} onOpenChange={onOpenChange} modal={false}>
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
            portalContainer={portalContainer}
            align="start"
            className="max-w-trim overflow-hidden"
            onFocusCapture={(event) => {
              // The lazy menu mounts after the opening keypress, before Radix can observe it.
              if (!open || event.target !== event.currentTarget || entryFocusedRef.current) return;
              entryFocusedRef.current = true;
              firstItemRef.current?.focus();
            }}
          >
            <DropdownMenuLabel>Workspace</DropdownMenuLabel>
            <DropdownMenuSeparator />
            <div
              className="max-h-80 overflow-y-auto"
              data-testid="workspace-list-scroll"
            >
              {sections.map((section) => (
                <WorkspaceSection
                  key={section.key}
                  section={section}
                  activeId={activeId}
                  firstMenuId={firstMenuId}
                  firstItemRef={firstItemRef}
                  onPick={pick}
                />
              ))}
            </div>
            {menuWorkspaces.length > 0 ? <DropdownMenuSeparator /> : null}
            <DropdownMenuItem
              ref={menuWorkspaces.length === 0 ? firstItemRef : undefined}
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
      </Nav>
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

type WorkspaceSectionProps = {
  section: SwitcherSection;
  activeId: string | null;
  // The menu's first item takes the entry focus. It sits in whichever section
  // leads, so every section is offered the ref and only that one keeps it.
  firstMenuId: string | null;
  firstItemRef: RefObject<HTMLDivElement | null>;
  onPick: (id: string) => void;
};

// One account's workspaces. A labelled section is a group its label names, so
// a screen reader says whose account an item is in; the one unlabelled section
// of a solo account adds no group.
function WorkspaceSection({ section, activeId, firstMenuId, firstItemRef, onPick }: WorkspaceSectionProps) {
  const labelId = useId();
  const items = section.workspaces.map((workspace) => {
    const label = workspace.name ?? WORKSPACE_LABEL.unnamed;
    return (
      <DropdownMenuItem
        key={workspace.id}
        ref={workspace.id === firstMenuId ? firstItemRef : undefined}
        onSelect={() => onPick(workspace.id)}
        data-active={workspace.id === activeId ? "true" : undefined}
      >
        <FolderIcon size={14} strokeWidth={1.75} aria-hidden="true" className="text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate" title={label}>
          {label}
        </span>
        {workspace.id === activeId ? <span aria-hidden="true">✓</span> : null}
      </DropdownMenuItem>
    );
  });
  if (section.label === null) return <>{items}</>;
  return (
    <DropdownMenuGroup aria-labelledby={labelId}>
      <DropdownMenuLabel id={labelId} variant="name">
        {section.label}
      </DropdownMenuLabel>
      {items}
    </DropdownMenuGroup>
  );
}
