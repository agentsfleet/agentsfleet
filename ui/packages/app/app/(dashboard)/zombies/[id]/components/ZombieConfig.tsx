"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@clerk/nextjs";
import { Loader2Icon, Trash2Icon } from "lucide-react";
import { buttonClassName } from "@usezombie/design-system";
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
  const { getToken } = useAuth();
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  async function onDelete() {
    setError(null);
    const token = await getToken();
    if (!token) {
      setError("Not authenticated");
      return;
    }
    startTransition(async () => {
      try {
        await deleteZombie(workspaceId, zombieId, token);
        router.push("/zombies");
        router.refresh();
      } catch (e) {
        const msg = e instanceof Error ? e.message : "Delete failed";
        setError(msg);
      }
    });
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

      {confirming ? (
        <div
          role="alertdialog"
          aria-labelledby="delete-zombie-title"
          className="rounded-md border border-destructive/40 bg-destructive/10 px-4 py-3"
        >
          <div id="delete-zombie-title" className="font-medium text-destructive">
            Delete {zombieName}?
          </div>
          <p className="mt-1 text-sm text-destructive/80">
            This removes the zombie. In-flight runs should be stopped first.
          </p>
          <div className="mt-3 flex gap-2">
            <button
              type="button"
              onClick={onDelete}
              disabled={pending}
              aria-busy={pending}
              className={buttonClassName("destructive", "sm")}
            >
              {pending ? (
                <Loader2Icon size={14} className="animate-spin" aria-hidden="true" />
              ) : (
                <Trash2Icon size={14} />
              )}
              {pending ? "Deleting…" : "Yes, delete"}
            </button>
            <button
              type="button"
              onClick={() => setConfirming(false)}
              disabled={pending}
              className={buttonClassName("ghost", "sm")}
            >
              Cancel
            </button>
          </div>
          {error ? (
            <div className="mt-2 text-xs text-destructive">{error}</div>
          ) : null}
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setConfirming(true)}
          className={buttonClassName("destructive", "sm")}
        >
          <Trash2Icon size={14} /> Delete zombie
        </button>
      )}
    </div>
  );
}
