"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import {
  Button,
  ConfirmDialog,
  DataTable,
  EmptyState,
  Spinner,
  type DataTableColumn,
} from "@agentsfleet/design-system";
import { KeyRoundIcon, PencilIcon, Trash2Icon } from "lucide-react";
import { deleteSecretAction } from "../actions";
import type { Secret } from "@/lib/api/secrets";
import { presentErrorString } from "@/lib/errors";
import EditSecretDialogDynamic from "@/components/domain/island-dynamic/EditSecretDialogDynamic";

type Props = {
  workspaceId: string;
  secrets: Secret[];
  protectedSecretName?: string | null;
};

const DATE_FORMATTER = new Intl.DateTimeFormat("en-US", {
  dateStyle: "medium",
  timeStyle: "short",
});

function formatCreatedAt(ms: number) {
  return DATE_FORMATTER.format(new Date(ms));
}

type SecretActionProps = {
  secret: Secret;
  pending: boolean;
  deleting: boolean;
  protectedFromDelete: boolean;
  onEdit: (name: string) => void;
  onDelete: (name: string) => void;
};

function SecretActions({
  secret,
  pending,
  deleting,
  protectedFromDelete,
  onEdit,
  onDelete,
}: SecretActionProps) {
  const deleteDisabled = pending || protectedFromDelete;
  return (
    <div className="flex justify-end gap-1">
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={() => onEdit(secret.name)}
        disabled={pending}
        aria-label={`Edit secret ${secret.name}`}
      >
        <PencilIcon size={14} />
      </Button>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={() => onDelete(secret.name)}
        disabled={deleteDisabled}
        aria-label={
          protectedFromDelete
            ? `Secret ${secret.name} is in model setup`
            : `Delete secret ${secret.name}`
        }
        title={
          protectedFromDelete
            ? "Switch model setup to platform defaults or another secret before deleting this one."
            : undefined
        }
      >
        {deleting ? <Spinner size="sm" srLabel="Deleting" /> : <Trash2Icon size={14} />}
      </Button>
    </div>
  );
}

function SecretNameCell({ secret }: { secret: Secret }) {
  return (
    <div className="min-w-0">
      <div className="truncate font-mono text-sm">{secret.name}</div>
      <div className="text-xs text-muted-foreground">Write-only encrypted secret</div>
    </div>
  );
}

function SecretCreatedCell({ secret }: { secret: Secret }) {
  return (
    <span className="font-mono text-xs tabular-nums text-muted-foreground">
      {formatCreatedAt(secret.created_at)}
    </span>
  );
}

function buildColumns({
  pending,
  target,
  protectedSecretName,
  onEdit,
  onDelete,
}: {
  pending: boolean;
  target: string | null;
  protectedSecretName: string | null;
  onEdit: (name: string) => void;
  onDelete: (name: string) => void;
}): DataTableColumn<Secret>[] {
  return [
    {
      key: "name",
      header: "Name",
      cell: (c) => <SecretNameCell secret={c} />,
    },
    {
      key: "created_at",
      header: "Created",
      hideOnMobile: true,
      cell: (c) => <SecretCreatedCell secret={c} />,
    },
    {
      key: "actions",
      header: "Actions",
      numeric: true,
      cell: (c) => (
        <SecretActions
          secret={c}
          pending={pending}
          deleting={pending && target === c.name}
          protectedFromDelete={protectedSecretName === c.name}
          onEdit={onEdit}
          onDelete={onDelete}
        />
      ),
    },
  ];
}

function SecretDialogs({
  workspaceId,
  editTarget,
  target,
  error,
  onEditClose,
  onDeleteClose,
  onConfirmDelete,
}: {
  workspaceId: string;
  editTarget: string | null;
  target: string | null;
  error: string | null;
  onEditClose: () => void;
  onDeleteClose: () => void;
  onConfirmDelete: (name: string) => void;
}) {
  return (
    <>
      <EditSecretDialogDynamic
        workspaceId={workspaceId}
        name={editTarget ?? ""}
        open={editTarget !== null}
        onOpenChange={onEditClose}
      />
      <ConfirmDialog
        open={target !== null}
        onOpenChange={onDeleteClose}
        title={`Delete secret "${target ?? ""}"?`}
        description="Deleting breaks fleets that reference it. This cannot be undone."
        confirmLabel="Delete"
        intent="destructive"
        errorMessage={error}
        onConfirm={() => {
          if (target) onConfirmDelete(target);
        }}
      />
    </>
  );
}

function SecretTable({
  secrets,
  pending,
  target,
  protectedSecretName,
  onEdit,
  onDelete,
}: {
  secrets: Secret[];
  pending: boolean;
  target: string | null;
  protectedSecretName: string | null;
  onEdit: (name: string) => void;
  onDelete: (name: string) => void;
}) {
  const columns = buildColumns({ pending, target, protectedSecretName, onEdit, onDelete });
  return (
    <DataTable
      columns={columns}
      rows={secrets}
      rowKey={(c) => c.name}
      caption="Stored secrets"
    />
  );
}

export default function SecretsList({
  workspaceId,
  secrets,
  protectedSecretName = null,
}: Props) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  const [target, setTarget] = useState<string | null>(null);
  const [editTarget, setEditTarget] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (secrets.length === 0) {
    return (
      <EmptyState
        icon={<KeyRoundIcon size={28} />}
        title="No secrets yet"
        description="Add a secret your fleets can use to reach other services."
      />
    );
  }

  function onConfirmDelete(name: string) {
    if (name === protectedSecretName) return;
    setError(null);
    startTransition(async () => {
      const result = await deleteSecretAction(workspaceId, name);
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: "delete the secret",
          }),
        );
        return;
      }
      setTarget(null);
      router.refresh();
    });
  }

  return (
    <div className="space-y-3">
      <SecretTable
        secrets={secrets}
        pending={pending}
        target={target}
        protectedSecretName={protectedSecretName}
        onEdit={(name) => {
          setError(null);
          setEditTarget(name);
        }}
        onDelete={(name) => {
          setError(null);
          setTarget(name);
        }}
      />
      <SecretDialogs
        workspaceId={workspaceId}
        editTarget={editTarget}
        target={target}
        error={error}
        onEditClose={() => setEditTarget(null)}
        onDeleteClose={() => {
          setTarget(null);
          setError(null);
        }}
        onConfirmDelete={onConfirmDelete}
      />
    </div>
  );
}
