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
import { deleteCredentialAction } from "../actions";
import type { CredentialSummary } from "@/lib/api/credentials";
import { presentErrorString } from "@/lib/errors";
import EditCredentialDialogDynamic from "@/components/domain/island-dynamic/EditCredentialDialogDynamic";

type Props = {
  workspaceId: string;
  credentials: CredentialSummary[];
  protectedCredentialName?: string | null;
};

const DATE_FORMATTER = new Intl.DateTimeFormat("en-US", {
  dateStyle: "medium",
  timeStyle: "short",
});

function formatCreatedAt(ms: number) {
  return DATE_FORMATTER.format(new Date(ms));
}

type CredentialActionProps = {
  credential: CredentialSummary;
  pending: boolean;
  deleting: boolean;
  protectedFromDelete: boolean;
  onEdit: (name: string) => void;
  onDelete: (name: string) => void;
};

function CredentialActions({
  credential,
  pending,
  deleting,
  protectedFromDelete,
  onEdit,
  onDelete,
}: CredentialActionProps) {
  const deleteDisabled = pending || protectedFromDelete;
  return (
    <div className="flex justify-end gap-1">
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={() => onEdit(credential.name)}
        disabled={pending}
        aria-label={`Edit credential ${credential.name}`}
      >
        <PencilIcon size={14} />
      </Button>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={() => onDelete(credential.name)}
        disabled={deleteDisabled}
        aria-label={
          protectedFromDelete
            ? `Credential ${credential.name} is in model setup`
            : `Delete credential ${credential.name}`
        }
        title={
          protectedFromDelete
            ? "Switch model setup to platform defaults or another credential before deleting this one."
            : undefined
        }
      >
        {deleting ? <Spinner size="sm" srLabel="Deleting" /> : <Trash2Icon size={14} />}
      </Button>
    </div>
  );
}

function CredentialNameCell({ credential }: { credential: CredentialSummary }) {
  return (
    <div className="min-w-0">
      <div className="truncate font-mono text-sm">{credential.name}</div>
      <div className="text-xs text-muted-foreground">Write-only encrypted secret</div>
    </div>
  );
}

function CredentialCreatedCell({ credential }: { credential: CredentialSummary }) {
  return (
    <span className="font-mono text-xs tabular-nums text-muted-foreground">
      {formatCreatedAt(credential.created_at)}
    </span>
  );
}

function buildColumns({
  pending,
  target,
  protectedCredentialName,
  onEdit,
  onDelete,
}: {
  pending: boolean;
  target: string | null;
  protectedCredentialName: string | null;
  onEdit: (name: string) => void;
  onDelete: (name: string) => void;
}): DataTableColumn<CredentialSummary>[] {
  return [
    {
      key: "name",
      header: "Name",
      cell: (c) => <CredentialNameCell credential={c} />,
    },
    {
      key: "created_at",
      header: "Created",
      hideOnMobile: true,
      cell: (c) => <CredentialCreatedCell credential={c} />,
    },
    {
      key: "actions",
      header: "Actions",
      numeric: true,
      cell: (c) => (
        <CredentialActions
          credential={c}
          pending={pending}
          deleting={pending && target === c.name}
          protectedFromDelete={protectedCredentialName === c.name}
          onEdit={onEdit}
          onDelete={onDelete}
        />
      ),
    },
  ];
}

function CredentialDialogs({
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
      <EditCredentialDialogDynamic
        workspaceId={workspaceId}
        name={editTarget ?? ""}
        open={editTarget !== null}
        onOpenChange={onEditClose}
      />
      <ConfirmDialog
        open={target !== null}
        onOpenChange={onDeleteClose}
        title={`Delete credential "${target ?? ""}"?`}
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

function CredentialTable({
  credentials,
  pending,
  target,
  protectedCredentialName,
  onEdit,
  onDelete,
}: {
  credentials: CredentialSummary[];
  pending: boolean;
  target: string | null;
  protectedCredentialName: string | null;
  onEdit: (name: string) => void;
  onDelete: (name: string) => void;
}) {
  const columns = buildColumns({ pending, target, protectedCredentialName, onEdit, onDelete });
  return (
    <DataTable
      columns={columns}
      rows={credentials}
      rowKey={(c) => c.name}
      caption="Stored credentials"
    />
  );
}

export default function CredentialsList({
  workspaceId,
  credentials,
  protectedCredentialName = null,
}: Props) {
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
        description="Add a secret your fleets can use to reach other services."
      />
    );
  }

  function onConfirmDelete(name: string) {
    if (name === protectedCredentialName) return;
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
    <div className="space-y-3">
      <CredentialTable
        credentials={credentials}
        pending={pending}
        target={target}
        protectedCredentialName={protectedCredentialName}
        onEdit={(name) => {
          setError(null);
          setEditTarget(name);
        }}
        onDelete={(name) => {
          setError(null);
          setTarget(name);
        }}
      />
      <CredentialDialogs
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
