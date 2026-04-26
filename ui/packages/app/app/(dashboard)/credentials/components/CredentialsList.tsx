"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { Button, ConfirmDialog } from "@usezombie/design-system";
import { Loader2Icon, Trash2Icon } from "lucide-react";
import { useClientToken } from "@/lib/auth/client";
import { deleteCredential, type CredentialSummary } from "@/lib/api/credentials";

type Props = {
  workspaceId: string;
  credentials: CredentialSummary[];
};

export default function CredentialsList({ workspaceId, credentials }: Props) {
  const router = useRouter();
  const { getToken } = useClientToken();
  const [pending, startTransition] = useTransition();
  const [target, setTarget] = useState<string | null>(null);

  if (credentials.length === 0) {
    return (
      <p className="text-sm text-zinc-500">
        No credentials stored yet. Add one using the form on the right.
      </p>
    );
  }

  function onConfirmDelete(name: string) {
    startTransition(async () => {
      const token = await getToken();
      if (!token) return;
      await deleteCredential(workspaceId, name, token);
      setTarget(null);
      router.refresh();
    });
  }

  return (
    <div className="divide-y rounded-md border">
      {credentials.map((c) => (
        <div key={c.name} className="flex items-center justify-between p-3">
          <div>
            <div className="font-mono text-sm">{c.name}</div>
            <div className="text-xs text-zinc-500">{c.created_at}</div>
          </div>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => setTarget(c.name)}
            disabled={pending}
            aria-label={`Delete credential ${c.name}`}
          >
            {pending && target === c.name ? (
              <Loader2Icon size={14} className="animate-spin" />
            ) : (
              <Trash2Icon size={14} />
            )}
          </Button>
        </div>
      ))}
      <ConfirmDialog
        open={target !== null}
        onOpenChange={(open) => {
          if (!open) setTarget(null);
        }}
        title={`Delete credential "${target ?? ""}"?`}
        description="Zombies referencing this name will fail to resolve until it is re-added. This cannot be undone."
        confirmLabel="Delete"
        intent="destructive"
        onConfirm={() => {
          if (target) onConfirmDelete(target);
        }}
      />
    </div>
  );
}
