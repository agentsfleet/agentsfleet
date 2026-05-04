"use client";

import { useState, useOptimistic, useTransition } from "react";
import { useRouter } from "next/navigation";
import { useClientToken } from "@/lib/auth/client";
import { Button, ConfirmDialog } from "@usezombie/design-system";
import { setZombieStatus } from "@/lib/api/zombies";
import { ApiError } from "@/lib/api/errors";
import type { Zombie, ZombieStatus } from "@/lib/api/zombies";

interface KillSwitchProps {
  workspaceId: string;
  zombie: Zombie;
}

interface ActionConfig {
  target: ZombieStatus;
  buttonLabel: string;
  variant: "outline" | "destructive";
  dialogTitle: string;
  dialogDescription: string;
  confirmLabel: string;
  intent: "default" | "destructive";
}

// Drives the per-zombie lifecycle controls. The panel renders a state-aware
// set of actions:
//   - `active`              → Stop (graceful halt) + Kill (terminal)
//   - `paused` (auto-halt)  → Resume + Kill
//   - `stopped` (op halt)   → Resume + Kill
//   - `killed` (terminal)   → no actions (DELETE is offered in ZombieConfig)
export default function KillSwitch({ workspaceId, zombie }: KillSwitchProps) {
  const { getToken } = useClientToken();
  const router = useRouter();
  const [pendingAction, setPendingAction] = useState<ActionConfig | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [optimisticStatus, setOptimisticStatus] = useOptimistic(zombie.status);
  const [, startTransition] = useTransition();

  async function handleConfirm() {
    if (!pendingAction) return;
    const action = pendingAction;
    const token = await getToken();
    if (!token) return;
    setErrorMessage(null);

    startTransition(async () => {
      const previous = optimisticStatus;
      setOptimisticStatus(action.target);
      try {
        await setZombieStatus(workspaceId, zombie.id, action.target, token);
        setPendingAction(null);
        router.refresh();
      } catch (err) {
        setOptimisticStatus(previous);
        if (err instanceof ApiError && err.status === 409) {
          // Status changed under us. Refresh picks up the new state.
          setPendingAction(null);
          router.refresh();
        } else {
          setErrorMessage(
            err instanceof ApiError
              ? err.message
              : `Failed to ${action.confirmLabel.toLowerCase()} zombie. Please try again.`,
          );
        }
      }
    });
  }

  const stopAction: ActionConfig = {
    target: "stopped",
    buttonLabel: "Stop",
    variant: "destructive",
    dialogTitle: "Stop this zombie?",
    dialogDescription: "Halt execution now. You can resume it later from this page or via the CLI.",
    confirmLabel: "Stop",
    intent: "destructive",
  };
  const resumeAction: ActionConfig = {
    target: "active",
    buttonLabel: "Resume",
    variant: "outline",
    dialogTitle: "Resume this zombie?",
    dialogDescription:
      optimisticStatus === "paused"
        ? "This zombie was auto-paused by the platform. Resuming returns it to active execution — investigate the trigger first."
        : "Return this zombie to active execution.",
    confirmLabel: "Resume",
    intent: "default",
  };
  const killAction: ActionConfig = {
    target: "killed",
    buttonLabel: "Kill",
    variant: "destructive",
    dialogTitle: "Kill this zombie permanently?",
    dialogDescription:
      "Marks the zombie terminal. This is irreversible — once killed, the zombie cannot be resumed and only Delete remains.",
    confirmLabel: "Kill",
    intent: "destructive",
  };

  const actions: ActionConfig[] = (() => {
    switch (optimisticStatus) {
      case "active":
        return [stopAction, killAction];
      case "paused":
      case "stopped":
        return [resumeAction, killAction];
      case "killed":
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
          setErrorMessage(err instanceof Error ? err.message : "An error occurred")
        }
        errorMessage={errorMessage}
      />
    </>
  );
}
