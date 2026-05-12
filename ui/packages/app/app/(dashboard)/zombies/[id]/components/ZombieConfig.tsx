"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Trash2Icon } from "lucide-react";
import { Button, ConfirmDialog } from "@usezombie/design-system";
import { deleteZombieAction } from "../../actions";
import { presentErrorString } from "@/lib/errors";

type Props = {
  workspaceId: string;
  zombieId: string;
  zombieName: string;
};

export default function ZombieConfig({
  workspaceId,
  zombieId,
  zombieName,
}: Props) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function onConfirm() {
    setError(null);
    const result = await deleteZombieAction(workspaceId, zombieId);
    if (!result.ok) {
      throw new Error(
        presentErrorString({
          errorCode: result.errorCode,
          message: result.error,
          action: "delete this zombie",
        }),
      );
    }
    // No router.refresh() — calling refresh immediately after push races
    // the URL commit (same surface InstallZombieForm hit); /zombies is
    // `force-dynamic` so it re-fetches on its own.
    router.push("/zombies");
  }

  return (
    <div className="rounded-md border border-border bg-card p-4">
      <p className="mb-4 text-sm text-muted-foreground">
        Rename, pause, and resume become available once the backend adds{" "}
        <code className="font-mono text-xs">PATCH</code> /{" "}
        <code className="font-mono text-xs">:pause</code> /{" "}
        <code className="font-mono text-xs">:resume</code> endpoints. Delete
        works today.
      </p>

      <Button
        type="button"
        onClick={() => setOpen(true)}
        variant="destructive"
        size="sm"
      >
        <Trash2Icon size={14} /> Delete zombie
      </Button>

      <ConfirmDialog
        open={open}
        onOpenChange={setOpen}
        title={`Delete ${zombieName}?`}
        description="This removes the zombie. In-flight runs should be stopped first."
        confirmLabel="Yes, delete"
        intent="destructive"
        onConfirm={onConfirm}
        errorMessage={error}
        onError={(e) =>
          setError(
            e instanceof Error
              ? e.message
              : presentErrorString({ action: "delete this zombie" }),
          )
        }
      />
    </div>
  );
}
