"use client";

import { useState, useOptimistic, useTransition } from "react";
import type { ReactNode } from "react";
import { useRouter } from "next/navigation";
import { ArchiveIcon, EllipsisIcon, Trash2Icon } from "lucide-react";
import { Button, ConfirmDialog, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { deleteFleetAction, setFleetStatusAction } from "../../actions";
import { presentErrorString } from "@/lib/errors";
import { workspacePath } from "@/lib/workspace-routes";
import { DELETE_MEMORY_TRAP_NOTICE } from "./console-copy";

interface FleetActionsProps {
  children?: ReactNode;
  workspaceId: string;
  fleet: {id: string; name: string; status: string; etag: string};
}

interface ActionConfig {
  kind: "archive" | "delete";
  buttonLabel: string;
  dialogTitle: string;
  dialogDescription: string;
  confirmLabel: string;
  // Static phrase fed to presentError so the error sentence reads
  // naturally per action. Kept as a string literal — never built from
  // confirmLabel at the call site (RULE UFS — verb literals stay
  // adjacent to the config that owns them).
  errorVerb: "archive this fleet" | "delete this fleet";
}

// Drives the per-fleet lifecycle controls. Both are terminal, so both confirm:
//   - `installing` / `active` → Archive (cancel live work, keep history) + Delete
//   - `archived` (terminal)   → Delete
// Delete works in any status; the daemon ends live work itself, so nothing is
// archived first.
export default function FleetActions({ workspaceId, fleet, children }: FleetActionsProps) {
  const router = useRouter();
  const [pendingAction, setPendingAction] = useState<ActionConfig | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [optimisticStatus, setOptimisticStatus] = useOptimistic(fleet.status);
  const [, startTransition] = useTransition();

  function reportFailure(action: ActionConfig, error: unknown) {
    setErrorMessage(presentErrorString({ message: error instanceof Error ? error.message : "", action: action.errorVerb }));
  }

  async function confirmDelete(action: ActionConfig) {
    try {
      const result = await deleteFleetAction(workspaceId, fleet.id, fleet.etag);
      if (result.ok) {
        setPendingAction(null);
        // No router.refresh() — refresh right after push races the URL
        // commit; the fleets list is `force-dynamic` and re-fetches itself.
        router.push(workspacePath(workspaceId, "fleets"));
        return;
      }
      setErrorMessage(presentErrorString({ errorCode: result.errorCode, message: result.error, action: action.errorVerb }));
    } catch (error) {
      reportFailure(action, error);
    }
  }

  async function confirmArchive(action: ActionConfig) {
    const previous = optimisticStatus;
    await new Promise<void>((resolve) => {
      startTransition(async () => {
        setOptimisticStatus(AGENTSFLEET_STATUS.ARCHIVED);
        try {
          const result = await setFleetStatusAction(workspaceId, fleet.id, AGENTSFLEET_STATUS.ARCHIVED, fleet.etag);
          if (result.ok) {
            setPendingAction(null);
            router.refresh();
            return;
          }
          setOptimisticStatus(previous);
          if (result.status === 409 || result.status === 412) {
            // Status changed under us (already archived, or a newer
            // version). Refresh picks up the new state.
            setPendingAction(null);
            router.refresh();
            return;
          }
          setErrorMessage(presentErrorString({ errorCode: result.errorCode, message: result.error, action: action.errorVerb }));
        } catch (error) {
          setOptimisticStatus(previous);
          reportFailure(action, error);
        } finally {
          resolve();
        }
      });
    });
  }

  // `action` is supplied by the dialog's onConfirm closure (bound only while
  // pendingAction is non-null), so no in-function null check is needed.
  async function handleConfirm(action: ActionConfig) {
    setErrorMessage(null);
    await (action.kind === "delete" ? confirmDelete(action) : confirmArchive(action));
  }

  const archiveAction: ActionConfig = {
    kind: "archive",
    buttonLabel: "Archive",
    dialogTitle: "Archive this fleet?",
    dialogDescription:
      "Cancels its live work and keeps its chat and activity. An archived fleet never runs again, and this cannot be undone.",
    confirmLabel: "Archive",
    errorVerb: "archive this fleet",
  };
  const deleteAction: ActionConfig = {
    kind: "delete",
    buttonLabel: "Delete fleet",
    dialogTitle: `Delete ${fleet.name}?`,
    dialogDescription: `This ends any live work and permanently deletes the fleet with its chat and activity. ${DELETE_MEMORY_TRAP_NOTICE}`,
    confirmLabel: "Delete fleet",
    errorVerb: "delete this fleet",
  };
  const actions: ActionConfig[] =
    optimisticStatus === AGENTSFLEET_STATUS.ARCHIVED ? [deleteAction] : [archiveAction, deleteAction];

  return (
    <>
        <DropdownMenu>
          <DropdownMenuTrigger asChild><Button variant="ghost" size="icon-compact" aria-label="Fleet actions"><EllipsisIcon aria-hidden="true" /></Button></DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-80 max-w-full p-sm">
            {actions.map((action) => <DropdownMenuItem key={action.kind} onSelect={() => setPendingAction(action)}
              className={action.kind === "delete" ? "text-destructive focus:text-destructive" : undefined}>
              {action.kind === "delete" ? <Trash2Icon aria-hidden="true" /> : <ArchiveIcon aria-hidden="true" />}
              {action.buttonLabel}
            </DropdownMenuItem>)}
            {children}
          </DropdownMenuContent>
        </DropdownMenu>
      <ConfirmDialog
        open={pendingAction !== null}
        // ConfirmDialog only calls onOpenChange on user-dismiss (cancel,
        // escape, click-outside) — `open={pendingAction !== null}` is the
        // controlled prop for the open case, so a dismiss-only handler
        // captures the full event surface without a `next` guard.
        onOpenChange={() => setPendingAction(null)}
        intent="destructive"
        title={pendingAction?.dialogTitle ?? ""}
        description={pendingAction?.dialogDescription ?? ""}
        confirmLabel={pendingAction?.confirmLabel ?? "Confirm"}
        // Bound to the pending action when open; `undefined` when closed so the
        // handler never needs an unreachable null guard.
        onConfirm={pendingAction ? () => handleConfirm(pendingAction) : undefined}
        // handleConfirm owns result and transport error reporting, and its
        // returned promise keeps both dialog actions disabled until settled.
        errorMessage={errorMessage}
      />
    </>
  );
}
