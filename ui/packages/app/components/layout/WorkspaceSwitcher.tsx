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
import { setActiveWorkspace } from "@/app/(dashboard)/actions";

type Props = {
  workspaces: TenantWorkspace[];
  activeId: string | null;
};

export default function WorkspaceSwitcher({ workspaces, activeId }: Props) {
  const [pending, startTransition] = useTransition();

  if (workspaces.length === 0) return null;

  const active = workspaces.find((w) => w.id === activeId) ?? workspaces[0];
  const activeLabel = active?.name ?? active?.id ?? "—";

  function pick(id: string) {
    if (id === activeId) return;
    startTransition(async () => {
      await setActiveWorkspace(id);
    });
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className="mc-ws-switcher"
        aria-label="Select workspace"
        disabled={pending}
      >
        <span className="mc-ws-label">{activeLabel}</span>
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
            <span className="mc-ws-item-name">{ws.name ?? ws.id}</span>
            {ws.id === active?.id ? <span aria-hidden="true">✓</span> : null}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
      <style>{`
        .mc-ws-switcher {
          display: inline-flex;
          align-items: center;
          gap: 0.4rem;
          padding: 0.35rem 0.7rem;
          margin-right: 0.75rem;
          border: 1px solid var(--z-border, rgba(255,255,255,0.1));
          border-radius: var(--z-radius-pill, 9999px);
          background: transparent;
          color: var(--z-text-primary);
          font-size: 0.82rem;
          cursor: pointer;
          transition: border-color 0.15s, background 0.15s;
        }
        .mc-ws-switcher:hover:not(:disabled) {
          border-color: var(--z-orange);
          background: rgba(255, 137, 0, 0.06);
        }
        .mc-ws-switcher:disabled { opacity: 0.6; cursor: wait; }
        .mc-ws-label { max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
        .mc-ws-item-name { flex: 1; }
      `}</style>
    </DropdownMenu>
  );
}
