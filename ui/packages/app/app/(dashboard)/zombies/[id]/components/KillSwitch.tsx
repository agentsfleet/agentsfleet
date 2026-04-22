"use client";

import { useState, useOptimistic, useTransition } from "react";
import { useRouter } from "next/navigation";
import { useClientToken } from "@/lib/auth/client";
import { Button, ConfirmDialog } from "@usezombie/design-system";
import { stopZombie } from "@/lib/api/zombies";
import { ApiError } from "@/lib/api/errors";
import type { Zombie } from "@/lib/api/zombies";

interface KillSwitchProps {
  workspaceId: string;
  zombie: Zombie;
}

export default function KillSwitch({ workspaceId, zombie }: KillSwitchProps) {
  const { getToken } = useClientToken();
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [optimisticStatus, setOptimisticStatus] = useOptimistic(zombie.status);
  const [, startTransition] = useTransition();

  const isStopped = optimisticStatus === "stopped";

  async function handleConfirm() {
    const token = await getToken();
    if (!token) return;
    setErrorMessage(null);

    startTransition(async () => {
      setOptimisticStatus("stopped");
      try {
        await stopZombie(workspaceId, zombie.id, token);
        setOpen(false);
        router.refresh();
      } catch (err) {
        setOptimisticStatus(zombie.status);
        if (err instanceof ApiError && err.status === 409) {
          // Zombie was already stopped elsewhere — refresh picks up the
          // updated status; no error surface needed.
          setOpen(false);
          router.refresh();
        } else {
          setErrorMessage(
            err instanceof ApiError
              ? err.message
              : "Failed to stop zombie. Please try again.",
          );
        }
      }
    });
  }

  if (isStopped) {
    return (
      <Button variant="outline" size="sm" disabled>
        Stopped
      </Button>
    );
  }

  return (
    <>
      <Button variant="destructive" size="sm" onClick={() => setOpen(true)}>
        Kill Switch
      </Button>
      <ConfirmDialog
        open={open}
        onOpenChange={setOpen}
        intent="destructive"
        title="Stop this zombie?"
        description="This will immediately halt execution. You can restart it later from the CLI."
        confirmLabel="Stop"
        onConfirm={handleConfirm}
        onError={(err) =>
          setErrorMessage(err instanceof Error ? err.message : "An error occurred")
        }
        errorMessage={errorMessage}
      />
    </>
  );
}
