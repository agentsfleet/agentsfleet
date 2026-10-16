"use client";

import { type RefObject, useEffect, useId, useRef, useState } from "react";
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
import CreateWorkspaceDialogDynamic from "@/components/domain/island-dynamic/CreateWorkspaceDialogDynamic";
import { useWorkspaceMenu } from "./workspace-menu-state";
import { type SwitcherSection, WORKSPACE_LABEL } from "./workspace-groups";
import { AccountMenuNavigation, MENU_VIEW, type MenuView } from "./AccountMenuNavigation";
import { WorkspaceSwitcherTrigger } from "./WorkspaceSwitcherTrigger";

type WorkspaceSwitcherMenuProps = {
  operatorScopes?: string[];
  collapsed?: boolean;
  open: boolean;
  workspaces: TenantWorkspace[];
  onOpenChange: (open: boolean) => void;
};

export default function WorkspaceSwitcherMenu({ open, operatorScopes = [], collapsed = false, workspaces, onOpenChange }: WorkspaceSwitcherMenuProps) {
  const [view, setView] = useState<MenuView>(MENU_VIEW.home);
  const menu = useWorkspaceMenu(workspaces);
  const [portalContainer, setPortalContainer] = useState<HTMLElement | null>(null);
  const switcherTriggerRef = useRef<HTMLButtonElement>(null);
  const firstItemRef = useRef<HTMLDivElement>(null);
  const entryFocusedRef = useRef(false);
  useEffect(() => {
    entryFocusedRef.current = false;
    if (!open) setView(MENU_VIEW.home);
    else if (view === MENU_VIEW.home) firstItemRef.current?.focus();
  }, [open, view]);
  return <>
    <Nav ref={setPortalContainer} aria-label="Workspaces" className="flex w-full min-w-0 items-center">
      <DropdownMenu open={open} onOpenChange={onOpenChange} modal={false}>
        <DropdownMenuTrigger asChild>
          <WorkspaceSwitcherTrigger ref={switcherTriggerRef} collapsed={collapsed} activeLabel={menu.activeLabel} busy={menu.pending}
            aria-label="Workspace and account menu" data-testid="workspace-switcher" disabled={menu.pending} />
        </DropdownMenuTrigger>
        <DropdownMenuContent portalContainer={portalContainer} align="start" side="top"
          className="w-72 max-w-[var(--radix-dropdown-menu-content-available-width)] max-h-[var(--radix-dropdown-menu-content-available-height)] overflow-y-auto p-sm"
          onFocusCapture={(event) => {
            // Lazy mounting misses Radix's original opening keypress.
            if (!open || event.target !== event.currentTarget || entryFocusedRef.current) return;
            entryFocusedRef.current = true;
            firstItemRef.current?.focus();
          }}>
          {view === MENU_VIEW.home ? <WorkspaceChoices menu={menu} firstItemRef={firstItemRef} /> : null}
          <AccountMenuNavigation view={view} onView={setView} workspaceId={menu.activeId} operatorScopes={operatorScopes} />
        </DropdownMenuContent>
      </DropdownMenu>
    </Nav>
    <CreateWorkspaceDialogDynamic open={menu.createOpen} pending={menu.creation.pending} error={menu.creation.error}
      onOpenChange={menu.setCreateDialogOpen} onSubmit={menu.creation.create} restoreFocus={() => switcherTriggerRef.current?.focus()} />
  </>;
}

function WorkspaceChoices({ menu, firstItemRef }: { menu: ReturnType<typeof useWorkspaceMenu>; firstItemRef: RefObject<HTMLDivElement | null> }) {
  return <>
    <DropdownMenuLabel>Switch workspace</DropdownMenuLabel>
    <DropdownMenuSeparator />
    <div className="max-h-48 overflow-y-auto" data-testid="workspace-list-scroll">
      {menu.sections.map((section) => <WorkspaceSection key={section.key} section={section} activeId={menu.activeId}
        firstMenuId={menu.firstMenuId} firstItemRef={firstItemRef} onPick={menu.pick} />)}
    </div>
    {menu.menuWorkspaces.length > 0 ? <DropdownMenuSeparator /> : null}
    <DropdownMenuItem ref={menu.menuWorkspaces.length === 0 ? firstItemRef : undefined}
      onSelect={() => menu.setCreateDialogOpen(true)} disabled={menu.creation.locked}
      aria-disabled={menu.creation.locked || undefined} data-testid="workspace-new">
      <PlusIcon size={14} aria-hidden="true" /><span className="flex-1">Create workspace</span>
    </DropdownMenuItem>
  </>;
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
