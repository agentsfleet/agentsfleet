"use client";

import { useEffect, useId, useRef, useState } from "react";
import { CheckIcon, FolderIcon, PlusIcon } from "@phosphor-icons/react/ssr";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  DropdownMenuSub,
  DropdownMenuSubTrigger,
  DropdownMenuSubContent,
  Nav,
} from "@agentsfleet/design-system";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import type { TenantBilling } from "@/lib/types";
import CreateWorkspaceDialogDynamic from "@/components/domain/island-dynamic/CreateWorkspaceDialogDynamic";
import { useWorkspaceMenu } from "./workspace-menu-state";
import { type SwitcherSection, WORKSPACE_LABEL } from "./workspace-groups";
import { AccountMenuNavigation } from "./AccountMenuNavigation";
import { WorkspaceSwitcherTrigger } from "./WorkspaceSwitcherTrigger";
import { useCurrentUser } from "@/lib/auth/client";
import { NAV_ICON_SIZE } from "./sidebar-navigation-data";

type WorkspaceSwitcherMenuProps = {

  collapsed?: boolean;
  open: boolean;
  workspaces: TenantWorkspace[];
  billing?: TenantBilling | null;
  onOpenChange: (open: boolean) => void;
};

export default function WorkspaceSwitcherMenu({ open, collapsed = false, workspaces, onOpenChange, billing }: WorkspaceSwitcherMenuProps) {
  const menu = useWorkspaceMenu(workspaces);
  const [portalContainer, setPortalContainer] = useState<HTMLElement | null>(null);
  const switcherTriggerRef = useRef<HTMLButtonElement>(null);
  const firstItemRef = useRef<HTMLDivElement>(null);
  const entryFocusedRef = useRef(false);
  useEffect(() => {
    entryFocusedRef.current = false;
    if (open) firstItemRef.current?.focus();
  }, [open]);
  return <>
    <Nav ref={setPortalContainer} aria-label="Workspaces" className="-mx-sm flex w-[calc(100%+var(--sp-md))] min-w-0 items-center md:-mx-lg md:w-[calc(100%+var(--sp-2xl))]">
      <DropdownMenu open={open} onOpenChange={onOpenChange} modal={false}>
        <DropdownMenuTrigger asChild>
          <WorkspaceSwitcherTrigger ref={switcherTriggerRef} collapsed={collapsed} activeLabel={menu.activeLabel} busy={menu.pending}
            aria-label="Workspace and account menu" data-testid="workspace-switcher" disabled={menu.pending} />
        </DropdownMenuTrigger>
        <DropdownMenuContent portalContainer={portalContainer} align="end" side="right" sideOffset={8}
          className="w-61 max-w-[var(--radix-dropdown-menu-content-available-width)] max-h-[var(--radix-dropdown-menu-content-available-height)] overflow-y-auto p-sm"
          onFocusCapture={(event) => {
            // Lazy mounting misses Radix's original opening keypress.
            if (!open || event.target !== event.currentTarget || entryFocusedRef.current) return;
            entryFocusedRef.current = true;
            firstItemRef.current?.focus();
          }}>
          <DropdownMenuSub>
            <DropdownMenuSubTrigger ref={firstItemRef} aria-label={`Switch workspace: ${menu.activeLabel}`} className="mb-md min-h-16 gap-lg px-xl py-lg">
              <AccountIdentity workspace={menu.activeLabel} />
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent portalContainer={portalContainer} aria-label="Switch workspace" className="w-61 max-h-[var(--radix-dropdown-menu-content-available-height)] overflow-y-auto p-md">
              <WorkspaceChoices menu={menu} />
            </DropdownMenuSubContent>
          </DropdownMenuSub>
          <AccountMenuNavigation workspaceId={menu.activeId} billing={billing} />
        </DropdownMenuContent>
      </DropdownMenu>
    </Nav>
    <CreateWorkspaceDialogDynamic open={menu.createOpen} pending={menu.creation.pending} error={menu.creation.error}
      onOpenChange={menu.setCreateDialogOpen} onSubmit={menu.creation.create} restoreFocus={() => switcherTriggerRef.current?.focus()} />
  </>;
}

function WorkspaceChoices({ menu }: { menu: ReturnType<typeof useWorkspaceMenu> }) {
  return <>
    <DropdownMenuLabel variant="name">Switch workspace</DropdownMenuLabel>
    <DropdownMenuSeparator />
    <div className="max-h-48 overflow-y-auto" data-testid="workspace-list-scroll">
      {menu.sections.map((section) => <WorkspaceSection key={section.key} section={section} activeId={menu.activeId}
        onPick={menu.pick} />)}
    </div>
    {menu.menuWorkspaces.length > 0 ? <DropdownMenuSeparator /> : null}
    <DropdownMenuItem
      onSelect={() => menu.setCreateDialogOpen(true)} disabled={menu.creation.locked}
      aria-disabled={menu.creation.locked || undefined} data-testid="workspace-new">
      <PlusIcon size={NAV_ICON_SIZE} className="shrink-0 text-muted-foreground" aria-hidden="true" /><span className="flex-1">Create workspace</span>
    </DropdownMenuItem>
  </>;
}

type WorkspaceSectionProps = {
  section: SwitcherSection;
  activeId: string | null;
  onPick: (id: string) => void;
};

// One account's workspaces. A labelled section is a group its label names, so
// a screen reader says whose account an item is in; the one unlabelled section
// of a solo account adds no group.
function WorkspaceSection({ section, activeId, onPick }: WorkspaceSectionProps) {
  const labelId = useId();
  const items = section.workspaces.map((workspace) => {
    const label = workspace.name ?? WORKSPACE_LABEL.unnamed;
    return (
      <DropdownMenuItem
        key={workspace.id}
        onSelect={() => onPick(workspace.id)}
        data-active={workspace.id === activeId ? "true" : undefined}
      >
        <FolderIcon size={NAV_ICON_SIZE} aria-hidden="true" className="shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate" title={label}>
          {label}
        </span>
        {workspace.id === activeId ? <CheckIcon size={14} aria-hidden="true" /> : null}
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

function AccountIdentity({workspace}: {workspace: string}) {
  const user = useCurrentUser();
  const name = user.displayName || user.emailAddress?.split("@")[0] || "Your account";
  const initials = name.trim().split(/\s+/).slice(0, 2).map(part => part[0]).join("").toUpperCase();
  return <>
    <span aria-hidden="true" className="flex size-8 shrink-0 items-center justify-center rounded-full bg-avatar text-label font-extrabold text-avatar-foreground">{initials}</span>
    <span className="min-w-0 flex-1 text-left"><span className="block truncate font-semibold">{name}</span><span className="mt-xs block truncate text-eyebrow font-normal text-text-dim">{workspace}</span></span>
  </>;
}
