"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { useClientToken } from "@/lib/auth/client";
import { Trash2Icon } from "lucide-react";
import { Button, ConfirmDialog } from "@usezombie/design-system";
import { deleteZombie } from "@/lib/api/zombies";

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
  const { getToken } = useClientToken();
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function onConfirm() {
    setError(null);
    const token = await getToken();
    if (!token) throw new Error("Not authenticated");
    await deleteZombie(workspaceId, zombieId, token);
    router.push("/zombies");
    router.refresh();
  }

  return (
    <div className="rounded-lg border border-border bg-card p-4">
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
        onError={(e) => setError(e instanceof Error ? e.message : "Delete failed")}
      />
    </div>
  );
}
