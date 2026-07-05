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
import { KeyRoundIcon, PencilIcon, PencilLineIcon, Trash2Icon } from "lucide-react";
import { deleteSecretAction } from "../actions";
import type { Secret } from "@/lib/api/secrets";
import { presentErrorString } from "@/lib/errors";
import EditSecretDialogDynamic from "@/components/domain/island-dynamic/EditSecretDialogDynamic";
import RenameSecretDialogDynamic from "@/components/domain/island-dynamic/RenameSecretDialogDynamic";

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
        variant="destructive"
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

function SecretNameCell({
  secret,
  pending,
  onRename,
}: {
  secret: Secret;
  pending: boolean;
  onRename: (name: string) => void;
}) {
  return (
    <div className="flex min-w-0 items-start gap-1">
      <div className="min-w-0">
        <div className="truncate font-mono text-sm">{secret.name}</div>
        <div className="text-xs text-muted-foreground">Write-only encrypted secret</div>
      </div>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={() => onRename(secret.name)}
        disabled={pending}
        aria-label={`Rename secret ${secret.name}`}
        title="Rename"
      >
        <PencilLineIcon size={14} />
      </Button>
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
  onRename,
  onDelete,
}: {
  pending: boolean;
  target: string | null;
  protectedSecretName: string | null;
  onEdit: (name: string) => void;
  onRename: (name: string) => void;
  onDelete: (name: string) => void;
}): DataTableColumn<Secret>[] {
  return [
    {
      key: "name",
      header: "Name",
      cell: (c) => <SecretNameCell secret={c} pending={pending} onRename={onRename} />,
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
  renameTarget,
  target,
  error,
  onEditClose,
  onRenameClose,
  onDeleteClose,
  onConfirmDelete,
}: {
  workspaceId: string;
  editTarget: string | null;
  renameTarget: string | null;
  target: string | null;
  error: string | null;
  onEditClose: () => void;
  onRenameClose: () => void;
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
      <RenameSecretDialogDynamic
        workspaceId={workspaceId}
        name={renameTarget ?? ""}
        open={renameTarget !== null}
        onOpenChange={onRenameClose}
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
  onRename,
  onDelete,
}: {
  secrets: Secret[];
  pending: boolean;
  target: string | null;
  protectedSecretName: string | null;
  onEdit: (name: string) => void;
  onRename: (name: string) => void;
  onDelete: (name: string) => void;
}) {
  const columns = buildColumns({ pending, target, protectedSecretName, onEdit, onRename, onDelete });
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
  const [renameTarget, setRenameTarget] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (secrets.length === 0) {
    return (
      <EmptyState
        icon={<KeyRoundIcon size={28} />}
        title="No secrets"
        description="Create secret to have your fleets reach other services securely."
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
        onRename={(name) => {
          setError(null);
          setRenameTarget(name);
        }}
        onDelete={(name) => {
          setError(null);
          setTarget(name);
        }}
      />
      <SecretDialogs
        workspaceId={workspaceId}
        editTarget={editTarget}
        renameTarget={renameTarget}
        target={target}
        error={error}
        onEditClose={() => setEditTarget(null)}
        onRenameClose={() => setRenameTarget(null)}
        onDeleteClose={() => {
          setTarget(null);
          setError(null);
        }}
        onConfirmDelete={onConfirmDelete}
      />
    </div>
  );
}
