"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { ChevronDownIcon, PlusIcon, SettingsIcon } from "lucide-react";
import {
  Button,
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
import CreateWorkspaceDialog from "./CreateWorkspaceDialog";

type Props = {
  workspaces: TenantWorkspace[];
  activeId: string | null;
  onSwitch: (id: string) => void | Promise<void>;
  showCreateButton?: boolean;
  showManageItem?: boolean;
};

const WORKSPACE_NOTICE_MS = 2800;

type Notice = {
  message: string;
  severity: ToastSeverity;
};

export default function WorkspaceSwitcher({
  workspaces,
  activeId,
  onSwitch,
  showCreateButton = false,
  showManageItem = true,
}: Props) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  const [createOpen, setCreateOpen] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const noticeTimer = useResettableTimeout();

  // Keep creation reachable when signup has not created a workspace yet.
  const active = workspaces.find((w) => w.id === activeId) ?? workspaces[0];
  const activeLabel = active?.name ?? active?.id ?? "No workspace";

  function workspaceLabel(id: string): string {
    const workspace = workspaces.find((w) => w.id === id);
    return workspace?.name ?? workspace?.id ?? id;
  }

  function showNotice(severity: ToastSeverity, message: string) {
    setNotice({ severity, message });
    noticeTimer.start(() => setNotice(null), WORKSPACE_NOTICE_MS);
  }

  function pick(id: string) {
    if (id === activeId) return;
    const label = workspaceLabel(id);
    startTransition(async () => {
      try {
        await onSwitch(id);
        router.refresh();
        showNotice("success", `Workspace changed to ${label}.`);
      } catch {
        showNotice("destructive", "Workspace switch failed.");
      }
    });
  }

  return (
    <>
      <div className="inline-flex flex-wrap items-center gap-2">
        <DropdownMenu>
          <DropdownMenuTrigger
            className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md border border-border bg-transparent text-foreground font-mono text-eyebrow cursor-pointer transition-colors duration-snap ease-snap enabled:hover:bg-muted disabled:opacity-60 disabled:cursor-wait"
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
            {showManageItem ? (
              <DropdownMenuItem onSelect={() => router.push("/settings")} data-testid="workspace-manage">
                <SettingsIcon size={14} aria-hidden="true" />
                <span className="flex-1">Manage workspace</span>
              </DropdownMenuItem>
            ) : null}
            <DropdownMenuItem onSelect={() => setCreateOpen(true)} data-testid="workspace-new">
              <PlusIcon size={14} aria-hidden="true" />
              <span className="flex-1">New workspace</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        {showCreateButton ? (
          <Button type="button" variant="secondary" onClick={() => setCreateOpen(true)}>
            <PlusIcon size={14} aria-hidden="true" />
            New workspace
          </Button>
        ) : null}
      </div>
      <CreateWorkspaceDialog
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
