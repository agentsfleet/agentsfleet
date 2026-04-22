"use client";

import { useTransition } from "react";
import { ChevronDownIcon } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from "@usezombie/design-system";
import type { TenantWorkspace } from "@/lib/api/workspaces";

type Props = {
  workspaces: TenantWorkspace[];
  activeId: string | null;
  onSwitch: (id: string) => void | Promise<void>;
};

export default function WorkspaceSwitcher({ workspaces, activeId, onSwitch }: Props) {
  const [pending, startTransition] = useTransition();

  if (workspaces.length === 0) return null;

  const active = workspaces.find((w) => w.id === activeId) ?? workspaces[0];
  const activeLabel = active?.name ?? active?.id ?? "—";

  function pick(id: string) {
    if (id === activeId) return;
    startTransition(async () => {
      await onSwitch(id);
    });
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className="inline-flex items-center gap-[0.4rem] px-[0.7rem] py-[0.35rem] mr-3 rounded-full border border-[color:var(--z-border,rgba(255,255,255,0.1))] bg-transparent text-[color:var(--z-text-primary)] text-[0.82rem] cursor-pointer transition-[border-color,background-color] duration-150 enabled:hover:border-[color:var(--z-orange)] enabled:hover:bg-[rgba(255,137,0,0.06)] disabled:opacity-60 disabled:cursor-wait"
        aria-label="Select workspace"
        disabled={pending}
      >
        <span className="max-w-[180px] overflow-hidden text-ellipsis whitespace-nowrap">
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
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
