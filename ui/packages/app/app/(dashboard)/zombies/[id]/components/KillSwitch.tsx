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
  errorVerb: "stop this zombie" | "resume this zombie" | "kill this zombie";
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

  async function handleConfirm() {
    if (!pendingAction) return;
    const action = pendingAction;
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
    dialogTitle: "Stop this zombie?",
    dialogDescription: "Halt execution now. You can resume it later from this page or via the CLI.",
    confirmLabel: "Stop",
    intent: "destructive",
    errorVerb: "stop this zombie",
  };
  const resumeAction: ActionConfig = {
    target: ZOMBIE_STATUS.ACTIVE,
    buttonLabel: "Resume",
    variant: "outline",
    dialogTitle: "Resume this zombie?",
    dialogDescription:
      optimisticStatus === ZOMBIE_STATUS.PAUSED
        ? "This zombie was auto-paused by the platform. Resuming returns it to active execution — investigate the trigger first."
        : "Return this zombie to active execution.",
    confirmLabel: "Resume",
    intent: "default",
    errorVerb: "resume this zombie",
  };
  const killAction: ActionConfig = {
    target: ZOMBIE_STATUS.KILLED,
    buttonLabel: "Kill",
    variant: "destructive",
    dialogTitle: "Kill this zombie permanently?",
    dialogDescription:
      "Marks the zombie terminal. This is irreversible — once killed, the zombie cannot be resumed and only Delete remains.",
    confirmLabel: "Kill",
    intent: "destructive",
    errorVerb: "kill this zombie",
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
        onOpenChange={(next) => {
          if (!next) setPendingAction(null);
        }}
        intent={pendingAction?.intent ?? "default"}
        title={pendingAction?.dialogTitle ?? ""}
        description={pendingAction?.dialogDescription ?? ""}
        confirmLabel={pendingAction?.confirmLabel ?? "Confirm"}
        onConfirm={handleConfirm}
        onError={(err) =>
          setErrorMessage(
            err instanceof Error
              ? err.message
              : presentErrorString({
                  action: pendingAction?.errorVerb ?? "complete this action",
                }),
          )
        }
        errorMessage={errorMessage}
      />
    </>
  );
}
