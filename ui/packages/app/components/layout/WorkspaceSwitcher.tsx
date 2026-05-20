"use client";

import { useState, useTransition } from "react";
import { ChevronDownIcon, PlusIcon } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from "@usezombie/design-system";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import CreateWorkspaceDialog from "./CreateWorkspaceDialog";

type Props = {
  workspaces: TenantWorkspace[];
  activeId: string | null;
  onSwitch: (id: string) => void | Promise<void>;
};

export default function WorkspaceSwitcher({ workspaces, activeId, onSwitch }: Props) {
  const [pending, startTransition] = useTransition();
  const [createOpen, setCreateOpen] = useState(false);

  // Render even with zero workspaces — that's exactly the case where the
  // operator needs the "New workspace" affordance (a tenant whose signup
  // webhook never bootstrapped one).
  const active = workspaces.find((w) => w.id === activeId) ?? workspaces[0];
  const activeLabel = active?.name ?? active?.id ?? "No workspace";

  function pick(id: string) {
    if (id === activeId) return;
    startTransition(async () => {
      await onSwitch(id);
    });
  }

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md border border-border bg-transparent text-foreground font-mono text-eyebrow cursor-pointer transition-colors duration-snap ease-snap enabled:hover:bg-muted disabled:opacity-60 disabled:cursor-wait"
          // aria-label is the static a11y handle; data-testid is the stable
          // structural selector for e2e specs (the visible text content is
          // the active workspace name and changes on every switch).
          aria-label="Select workspace"
          data-testid="workspace-switcher"
          disabled={pending}
        >
          <span className="max-w-trim overflow-hidden text-ellipsis whitespace-nowrap">
            {activeLabel}
          </span>
          <ChevronDownIcon size={14} aria-hidden="true" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
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
            <span className="flex-1">New workspace</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <CreateWorkspaceDialog open={createOpen} onOpenChange={setCreateOpen} />
    </>
  );
}
