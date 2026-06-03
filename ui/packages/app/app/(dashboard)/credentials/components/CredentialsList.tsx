"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { Button, ConfirmDialog, Spinner } from "@usezombie/design-system";
import { Trash2Icon } from "lucide-react";
import { deleteCredentialAction } from "../actions";
import type { CredentialSummary } from "@/lib/api/credentials";
import { presentErrorString } from "@/lib/errors";

type Props = {
  workspaceId: string;
  credentials: CredentialSummary[];
};

export default function CredentialsList({ workspaceId, credentials }: Props) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  const [target, setTarget] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (credentials.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No credentials stored yet. Add one using the form on the right.
      </p>
    );
  }

  function onConfirmDelete(name: string) {
    setError(null);
    startTransition(async () => {
      const result = await deleteCredentialAction(workspaceId, name);
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: "delete the credential",
          }),
        );
        return;
      }
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
            <div className="font-mono text-xs tabular-nums text-muted-foreground">{c.created_at}</div>
          </div>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => {
              setError(null);
              setTarget(c.name);
            }}
            disabled={pending}
            aria-label={`Delete credential ${c.name}`}
          >
            {pending && target === c.name ? (
              <Spinner size="sm" srLabel="Deleting" />
            ) : (
              <Trash2Icon size={14} />
            )}
          </Button>
        </div>
      ))}
      <ConfirmDialog
        open={target !== null}
        onOpenChange={(open) => {
          if (!open) {
            setTarget(null);
            setError(null);
          }
        }}
        title={`Delete credential "${target ?? ""}"?`}
        description="Agents referencing this name will fail to resolve until it is re-added. This cannot be undone."
        confirmLabel="Delete"
        intent="destructive"
        errorMessage={error}
        onConfirm={() => {
          if (target) onConfirmDelete(target);
        }}
      />
    </div>
  );
}
