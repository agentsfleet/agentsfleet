"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { Button, ConfirmDialog, EmptyState, Spinner } from "@usezombie/design-system";
import { KeyRoundIcon, PencilIcon, Trash2Icon } from "lucide-react";
import { deleteCredentialAction } from "../actions";
import type { CredentialSummary } from "@/lib/api/credentials";
import { presentErrorString } from "@/lib/errors";
import EditCredentialDialog from "./EditCredentialDialog";

type Props = {
  workspaceId: string;
  credentials: CredentialSummary[];
};

export default function CredentialsList({ workspaceId, credentials }: Props) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  const [target, setTarget] = useState<string | null>(null);
  const [editTarget, setEditTarget] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (credentials.length === 0) {
    return (
      <EmptyState
        icon={<KeyRoundIcon size={28} />}
        title="No credentials yet"
        description="Add a secret your agents can use to reach other services."
      />
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
          <div className="flex items-center gap-1">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => {
                setError(null);
                setEditTarget(c.name);
              }}
              disabled={pending}
              aria-label={`Edit credential ${c.name}`}
            >
              <PencilIcon size={14} />
            </Button>
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
        </div>
      ))}
      <EditCredentialDialog
        workspaceId={workspaceId}
        name={editTarget ?? ""}
        open={editTarget !== null}
        // The dialog is parent-controlled and only ever emits a close, so clear
        // the target unconditionally.
        onOpenChange={() => setEditTarget(null)}
      />
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
