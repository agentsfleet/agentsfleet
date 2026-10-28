"use client";

import { useState, useOptimistic, useTransition } from "react";
import type { ReactNode } from "react";
import { useRouter } from "next/navigation";
import { EllipsisIcon, PowerOffIcon, CircleStopIcon, PlayIcon } from "lucide-react";
import { Button, ConfirmDialog, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import type { FleetStatusSettable } from "@/lib/api/fleets";
import { setFleetStatusAction } from "../../actions";
import { presentErrorString } from "@/lib/errors";

interface KillSwitchProps {
  children?: ReactNode;
  workspaceId: string;
  fleet: {id: string; status: string; etag: string};
}

interface ActionConfig {
  target: FleetStatusSettable;
  buttonLabel: string;
  dialogTitle: string;
  dialogDescription: string;
  confirmLabel: string;
  intent: "default" | "destructive";
  // Static phrase fed to presentError so the error sentence reads
  // naturally per action. Kept as a string literal — never built from
  // confirmLabel at the call site (RULE UFS — verb literals stay
  // adjacent to the config that owns them).
  errorVerb: "stop this fleet" | "resume this fleet" | "kill this fleet";
}

// Drives the per-fleet lifecycle controls. The panel renders a state-aware
// set of actions:
//   - `active`              → Stop (graceful halt) + Kill (terminal)
//   - `paused` (auto-halt)  → Resume + Kill
//   - `stopped` (op halt)   → Resume + Kill
//   - `killed` (terminal)   → no actions (DELETE is offered in FleetConfig)
export default function KillSwitch({ workspaceId, fleet, children }: KillSwitchProps) {
  const router = useRouter();
  const [pendingAction, setPendingAction] = useState<ActionConfig | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [optimisticStatus, setOptimisticStatus] = useOptimistic(fleet.status);
  const [, startTransition] = useTransition();

  // `action` is supplied by the dialog's onConfirm closure (bound only while
  // pendingAction is non-null), so no in-function null check is needed.
  async function handleConfirm(action: ActionConfig) {
    setErrorMessage(null);
    const previous = optimisticStatus;

    await new Promise<void>((resolve) => {
      startTransition(async () => {
        setOptimisticStatus(action.target);
        try {
          const result = await setFleetStatusAction(workspaceId, fleet.id, action.target, fleet.etag);
          if (result.ok) {
            setPendingAction(null);
            router.refresh();
            return;
          }
          setOptimisticStatus(previous);
          if (result.status === 409 || result.status === 412) {
            // Status changed under us. Refresh picks up the new state.
            setPendingAction(null);
            router.refresh();
            return;
          }
          setErrorMessage(
            presentErrorString({
              errorCode: result.errorCode,
              message: result.error,
              action: action.errorVerb,
            }),
          );
        } catch (error) {
          setOptimisticStatus(previous);
          setErrorMessage(
            presentErrorString({
              message: error instanceof Error ? error.message : "",
              action: action.errorVerb,
            }),
          );
        } finally {
          resolve();
        }
      });
    });
  }

  const stopAction: ActionConfig = {
    target: AGENTSFLEET_STATUS.STOPPED,
    buttonLabel: "Stop",
    dialogTitle: "Stop this fleet?",
    dialogDescription: "Halt execution now. You can resume it later from this page or via the CLI.",
    confirmLabel: "Stop",
    intent: "destructive",
    errorVerb: "stop this fleet",
  };
  const resumeAction: ActionConfig = {
    target: AGENTSFLEET_STATUS.ACTIVE,
    buttonLabel: "Resume",
    dialogTitle: "Resume this fleet?",
    dialogDescription:
      optimisticStatus === AGENTSFLEET_STATUS.PAUSED
        ? "This fleet was auto-paused by the platform. Resuming returns it to active execution — investigate the trigger first."
        : "Return this fleet to active execution.",
    confirmLabel: "Resume",
    intent: "default",
    errorVerb: "resume this fleet",
  };
  const killAction: ActionConfig = {
    target: AGENTSFLEET_STATUS.KILLED,
    buttonLabel: "Kill",
    dialogTitle: "Kill this fleet permanently?",
    dialogDescription:
      "Marks the fleet terminal. This is irreversible — once killed, the fleet cannot be resumed and only Delete remains.",
    confirmLabel: "Kill",
    intent: "destructive",
    errorVerb: "kill this fleet",
  };

  const actions: ActionConfig[] = (() => {
    switch (optimisticStatus) {
      case AGENTSFLEET_STATUS.ACTIVE:
        return [stopAction, killAction];
      case AGENTSFLEET_STATUS.PAUSED:
      case AGENTSFLEET_STATUS.STOPPED:
        return [resumeAction, killAction];
      case AGENTSFLEET_STATUS.KILLED:
      default:
        return [];
    }
  })();

  return (
    <>
        <DropdownMenu>
          <DropdownMenuTrigger asChild><Button variant="ghost" size="icon-compact" aria-label="Fleet actions"><EllipsisIcon aria-hidden="true" /></Button></DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-80 max-w-full p-sm">
            {actions.map((action) => <DropdownMenuItem key={action.target} onSelect={() => setPendingAction(action)}
              className={action.target === AGENTSFLEET_STATUS.KILLED ? "text-destructive focus:text-destructive" : undefined}>
              {action.target === AGENTSFLEET_STATUS.KILLED ? <PowerOffIcon aria-hidden="true" /> : action.target === AGENTSFLEET_STATUS.STOPPED ? <CircleStopIcon aria-hidden="true" /> : <PlayIcon aria-hidden="true" />}
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
        intent={pendingAction?.intent ?? "default"}
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
