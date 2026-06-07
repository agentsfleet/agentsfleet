"use client";

import { useState, useOptimistic, useTransition } from "react";
import { useRouter } from "next/navigation";
import { Button, ConfirmDialog } from "@usezombie/design-system";
import { ZOMBIE_STATUS } from "@/lib/api/zombies";
import type { Zombie, ZombieStatusSettable } from "@/lib/api/zombies";
import { setZombieStatusAction } from "../../actions";
import { presentErrorString } from "@/lib/errors";

interface KillSwitchProps {
  workspaceId: string;
  zombie: Zombie;
}

interface ActionConfig {
  target: ZombieStatusSettable;
  buttonLabel: string;
  variant: "outline" | "destructive";
  dialogTitle: string;
  dialogDescription: string;
  confirmLabel: string;
  intent: "default" | "destructive";
  // Static phrase fed to presentError so the error sentence reads
  // naturally per action. Kept as a string literal — never built from
  // confirmLabel at the call site (RULE UFS — verb literals stay
  // adjacent to the config that owns them).
  errorVerb: "stop this agent" | "resume this agent" | "kill this agent";
}

// Drives the per-zombie lifecycle controls. The panel renders a state-aware
// set of actions:
//   - `active`              → Stop (graceful halt) + Kill (terminal)
//   - `paused` (auto-halt)  → Resume + Kill
//   - `stopped` (op halt)   → Resume + Kill
//   - `killed` (terminal)   → no actions (DELETE is offered in ZombieConfig)
export default function KillSwitch({ workspaceId, zombie }: KillSwitchProps) {
  const router = useRouter();
  const [pendingAction, setPendingAction] = useState<ActionConfig | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [optimisticStatus, setOptimisticStatus] = useOptimistic(zombie.status);
  const [, startTransition] = useTransition();

  // `action` is supplied by the dialog's onConfirm closure (bound only while
  // pendingAction is non-null), so no in-function null check is needed.
  async function handleConfirm(action: ActionConfig) {
    setErrorMessage(null);

    startTransition(async () => {
      const previous = optimisticStatus;
      setOptimisticStatus(action.target);
      const result = await setZombieStatusAction(workspaceId, zombie.id, action.target);
      if (result.ok) {
        setPendingAction(null);
        router.refresh();
        return;
      }
      setOptimisticStatus(previous);
      if (result.status === 409) {
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
    });
  }

  const stopAction: ActionConfig = {
    target: ZOMBIE_STATUS.STOPPED,
    buttonLabel: "Stop",
    variant: "destructive",
    dialogTitle: "Stop this agent?",
    dialogDescription: "Halt execution now. You can resume it later from this page or via the CLI.",
    confirmLabel: "Stop",
    intent: "destructive",
    errorVerb: "stop this agent",
  };
  const resumeAction: ActionConfig = {
    target: ZOMBIE_STATUS.ACTIVE,
    buttonLabel: "Resume",
    variant: "outline",
    dialogTitle: "Resume this agent?",
    dialogDescription:
      optimisticStatus === ZOMBIE_STATUS.PAUSED
        ? "This agent was auto-paused by the platform. Resuming returns it to active execution — investigate the trigger first."
        : "Return this agent to active execution.",
    confirmLabel: "Resume",
    intent: "default",
    errorVerb: "resume this agent",
  };
  const killAction: ActionConfig = {
    target: ZOMBIE_STATUS.KILLED,
    buttonLabel: "Kill",
    variant: "destructive",
    dialogTitle: "Kill this agent permanently?",
    dialogDescription:
      "Marks the agent terminal. This is irreversible — once killed, the agent cannot be resumed and only Delete remains.",
    confirmLabel: "Kill",
    intent: "destructive",
    errorVerb: "kill this agent",
  };

  const actions: ActionConfig[] = (() => {
    switch (optimisticStatus) {
      case ZOMBIE_STATUS.ACTIVE:
        return [stopAction, killAction];
      case ZOMBIE_STATUS.PAUSED:
      case ZOMBIE_STATUS.STOPPED:
        return [resumeAction, killAction];
      case ZOMBIE_STATUS.KILLED:
      default:
        return [];
    }
  })();

  if (actions.length === 0) {
    return (
      <Button variant="outline" size="sm" disabled>
        Killed
      </Button>
    );
  }

  return (
    <>
      <div className="flex items-center gap-2">
        {actions.map((action) => (
          <Button
            key={action.target}
            variant={action.variant}
            size="sm"
            onClick={() => setPendingAction(action)}
          >
            {action.buttonLabel}
          </Button>
        ))}
      </div>
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
        // handleConfirm owns its own error reporting (sets errorMessage
        // directly on result.ok=false). It never throws, so ConfirmDialog
        // doesn't need an onError backup.
        errorMessage={errorMessage}
      />
    </>
  );
}
