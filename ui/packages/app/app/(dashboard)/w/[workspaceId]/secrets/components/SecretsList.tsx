"use client";

import { useOptimistic, useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import {
  ConfirmDialog,
  CopyButton,
  DataTable,
  EmptyState,
  IconAction,
  Time,
  type DataTableColumn,
} from "@agentsfleet/design-system";
import { KeyRoundIcon, PencilIcon, PencilLineIcon, Trash2Icon } from "lucide-react";
import { deleteSecretAction } from "../actions";
import type { Secret } from "@/lib/api/secrets";
import { isDefiniteRefusal } from "@/lib/api/errors";
import { presentErrorString } from "@/lib/errors";
import EditSecretDialogDynamic from "@/components/domain/island-dynamic/EditSecretDialogDynamic";
import RenameSecretDialogDynamic from "@/components/domain/island-dynamic/RenameSecretDialogDynamic";
import { SECRET_ROW_DESCRIPTION } from "../copy";

type Props = {
  workspaceId: string;
  secrets: Secret[];
  protectedSecretName?: string | null;
};

type SecretActionProps = {
  secret: Secret;
  pending: boolean;
  protectedFromDelete: boolean;
  onEdit: (name: string) => void;
  onDelete: (name: string) => void;
};

function SecretActions({
  secret,
  pending,
  protectedFromDelete,
  onEdit,
  onDelete,
}: SecretActionProps) {
  const deleteDisabled = pending || protectedFromDelete;
  return (
    <div className="flex justify-end gap-1">
      <IconAction
        type="button"
        variant="ghost"
        onClick={() => onEdit(secret.name)}
        disabled={pending}
        label={`Edit secret ${secret.name}`}
      >
        <PencilIcon size={14} />
      </IconAction>
      <IconAction
        type="button"
        variant="destructive"
        onClick={() => onDelete(secret.name)}
        disabled={deleteDisabled}
        label={
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
        <Trash2Icon size={14} />
      </IconAction>
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
        {/* The name is the interpolation key a user retypes into fleet config as
            ${secrets.<name>.<field>}. Copying it removes a whole class of silent
            typo — a mistyped key resolves to nothing, and the fleet just fails. */}
        <div className="flex min-w-0 items-center gap-1">
          <div className="truncate font-mono text-sm">{secret.name}</div>
          <CopyButton value={secret.name} label={`Copy secret name: ${secret.name}`} />
        </div>
        <div className="text-xs text-muted-foreground">{SECRET_ROW_DESCRIPTION}</div>
      </div>
      <IconAction
        type="button"
        variant="ghost"
        onClick={() => onRename(secret.name)}
        disabled={pending}
        label={`Rename secret ${secret.name}`}
        title="Rename"
      >
        <PencilLineIcon size={14} />
      </IconAction>
    </div>
  );
}

function SecretCreatedCell({ secret }: { secret: Secret }) {
  return (
    <Time
      value={new Date(secret.created_at)}
      format="relative"
      className="font-mono text-xs tabular-nums text-muted-foreground"
    />
  );
}

function buildColumns({
  pending,
  protectedSecretName,
  onEdit,
  onRename,
  onDelete,
}: {
  pending: boolean;
  protectedSecretName: string | null;
  onEdit: (name: string) => void;
  onRename: (name: string) => void;
  onDelete: (name: string) => void;
}): DataTableColumn<Secret>[] {
  return [
    {
      key: "name",
      header: "Name",
      sortValue: (c) => c.name,
      cell: (c) => <SecretNameCell secret={c} pending={pending} onRename={onRename} />,
    },
    {
      key: "created_at",
      header: "Created",
      sortValue: (c) => c.created_at,
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
  existingNames,
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
  existingNames: readonly string[];
  target: string | null;
  error: string | null;
  onEditClose: () => void;
  onRenameClose: () => void;
  onDeleteClose: () => void;
  onConfirmDelete: (name: string) => Promise<void>;
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
        existingNames={existingNames}
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
        // The promise reaches the dialog so it can hold both buttons disabled
        // and read "Working…" until the delete has settled.
        onConfirm={target ? () => onConfirmDelete(target) : undefined}
      />
    </>
  );
}

function SecretTable({
  secrets,
  pending,
  protectedSecretName,
  onEdit,
  onRename,
  onDelete,
}: {
  secrets: Secret[];
  pending: boolean;
  protectedSecretName: string | null;
  onEdit: (name: string) => void;
  onRename: (name: string) => void;
  onDelete: (name: string) => void;
}) {
  const columns = buildColumns({ pending, protectedSecretName, onEdit, onRename, onDelete });
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
  // The row leaves the table the moment the operator confirms; the server is
  // told inside the same transition. A rejected delete ends that transition and
  // React restores the row from the server-rendered list on its own — the same
  // shape the fleet kill switch uses, so nothing here has to put it back.
  const [visibleSecrets, hideSecret] = useOptimistic(
    secrets,
    (current: Secret[], removedName: string) => current.filter((secret) => secret.name !== removedName),
  );

  // Resolves when the transition settles, so the dialog can hold its buttons
  // disabled and read "Working…" until then (the kill switch's shape). A
  // confirm that returned at once would leave the dialog live, and a second
  // click would send a second delete.
  function onConfirmDelete(name: string): Promise<void> {
    if (name === protectedSecretName) return Promise.resolve();
    setError(null);
    return new Promise<void>((resolve) => {
      startTransition(async () => {
        try {
          hideSecret(name);
          const result = await deleteSecretAction(workspaceId, name);
          if (!result.ok) {
            setError(
              presentErrorString({
                errorCode: result.errorCode,
                message: result.error,
                action: "delete the secret",
              }),
            );
            // A refusal the server made is a no-op: the row is back when the
            // transition ends. A failure whose outcome is unknown — a timeout,
            // a transport fault, a gateway error — may have deleted the row
            // anyway, and the re-read makes its return or absence server truth.
            if (!isDefiniteRefusal(result.status)) router.refresh();
            return;
          }
          setTarget(null);
          router.refresh();
        } finally {
          resolve();
        }
      });
    });
  }

  return (
    <div className="space-y-3">
      {/* "No secrets" is the server's claim to make: the optimistic hide of the
          last row keeps the table shell until the delete is answered, so the
          status region never announces an emptiness the server may retract. */}
      {secrets.length === 0 ? (
        <EmptyState
          icon={<KeyRoundIcon size={28} />}
          title="No secrets"
          description="Create secret to have your fleets reach other services securely."
        />
      ) : (
        <SecretTable
          secrets={visibleSecrets}
          pending={pending}
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
      )}
      <SecretDialogs
        workspaceId={workspaceId}
        editTarget={editTarget}
        renameTarget={renameTarget}
        existingNames={visibleSecrets.map((s) => s.name)}
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
