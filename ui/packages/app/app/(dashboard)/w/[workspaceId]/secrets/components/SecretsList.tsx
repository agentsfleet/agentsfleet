"use client";

import { useOptimistic, useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import {
  ConfirmDialog,
  DataTable,
  EmptyState,
  type DataTableColumn,
} from "@agentsfleet/design-system";
import { KeyIcon } from "@phosphor-icons/react/ssr";
import { deleteSecretAction } from "../actions";
import type { Secret } from "@/lib/api/secrets";
import { isDefiniteRefusal } from "@/lib/api/errors";
import { presentErrorString } from "@/lib/errors";
import EditSecretDialogDynamic from "@/components/domain/island-dynamic/EditSecretDialogDynamic";
import { SecretActions, SecretNameCell, secretType } from "./secret-row-cells";
import {
  DELETE_CONNECTION_CONFIRM,
  DELETE_SECRET_CONFIRM,
  DELETE_SECRET_DESCRIPTION,
  deleteConnectionDescription,
} from "../copy";

type Props = {
  workspaceId: string;
  secrets: Secret[];
  protectedSecretName?: string | null;
  /** Secret name → connector display name, for each live connection. */
  connections?: Readonly<Record<string, string>>;
};

function buildColumns({
  pending,
  protectedSecretName,
  onEdit,
  onDelete,
}: {
  pending: boolean;
  protectedSecretName: string | null;
  onEdit: (name: string) => void;
  onDelete: (name: string) => void;
}): DataTableColumn<Secret>[] {
  return [
    {key: "name", primary: true, header: "Name", cell: secret => <SecretNameCell secret={secret} />},
    {key: "type", header: "Type", cell: secret => secretType(secret)},
    {key: "value", header: "Value", cell: () => <span className="text-text-dim">Hidden</span>},
    {key: "scope", header: "Available to", cell: () => <span className="text-text-dim">Access details unavailable</span>},
    {key: "actions", header: <span className="sr-only">Actions</span>, numeric: true, cell: secret => <SecretActions secret={secret} pending={pending}
      protectedFromDelete={protectedSecretName === secret.name} onEdit={onEdit} onDelete={onDelete} />},
  ];
}

function SecretDialogs({
  workspaceId,
  editTarget,
  target,
  connection,
  error,
  onEditClose,
  onDeleteClose,
  onConfirmDelete,
}: {
  workspaceId: string;
  editTarget: string | null;
  target: string | null;
  connection: string | undefined;
  error: string | null;
  onEditClose: () => void;
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
      <ConfirmDialog
        open={target !== null}
        onOpenChange={onDeleteClose}
        title={`Delete secret “${target ?? ""}”?`}
        description={connection ? deleteConnectionDescription(connection) : DELETE_SECRET_DESCRIPTION}
        confirmLabel={connection ? DELETE_CONNECTION_CONFIRM : DELETE_SECRET_CONFIRM}
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
  onDelete,
}: {
  secrets: Secret[];
  pending: boolean;
  protectedSecretName: string | null;
  onEdit: (name: string) => void;
  onDelete: (name: string) => void;
}) {
  return <DataTable columns={buildColumns({pending, protectedSecretName, onEdit, onDelete})}
    rows={secrets} isLoading={pending} rowKey={secret => secret.name} caption="Stored secrets"
    className="[&_table]:min-w-160" pagination={false} />;
}

export default function SecretsList({
  workspaceId,
  secrets,
  protectedSecretName = null,
  connections = {},
}: Props) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  const [target, setTarget] = useState<string | null>(null);
  const [editTarget, setEditTarget] = useState<string | null>(null);
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
    <div className="flex min-h-0 flex-col gap-lg">
      {/* "No secrets" is the server's claim to make: the optimistic hide of the
          last row keeps the table shell until the delete is answered, so the
          status region never announces an emptiness the server may retract. */}
      {secrets.length === 0 ? (
        <EmptyState
          icon={<KeyIcon size={28} />}
          title="Keep your credentials in one place"
          description="Create a secret for the services your fleets need. Saved values are encrypted and stay hidden; you can replace them whenever needed."
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
          onDelete={(name) => {
            setError(null);
            setTarget(name);
          }}
        />
      )}
      <p className="text-eyebrow leading-eyebrow text-text-dim">Secret values are hidden after saving.</p>
      <SecretDialogs
        workspaceId={workspaceId}
        editTarget={editTarget}
        target={target}
        connection={target !== null && Object.hasOwn(connections, target) ? connections[target] : undefined}
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
