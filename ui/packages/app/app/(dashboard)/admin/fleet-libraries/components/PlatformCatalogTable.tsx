"use client";

import { useState, useTransition } from "react";
import {
  Badge,
  ConfirmDialog,
  DataTable,
  type DataTableColumn,
  EmptyState,
  IconAction,
} from "@agentsfleet/design-system";
import {
  DownloadIcon,
  EyeIcon,
  EyeOffIcon,
  LibraryIcon,
  PencilIcon,
  Trash2Icon,
} from "lucide-react";
import type { PlatformCatalogEntry } from "@/lib/types";
import { presentErrorString } from "@/lib/errors";
import { deletePlatformLibraryAction, patchPlatformLibraryAction } from "../actions";
import {
  COLUMN_ACTIONS,
  COLUMN_BUNDLE,
  COLUMN_FLEET,
  COLUMN_REPOSITORY,
  COLUMN_STATUS,
  DELETE,
  DELETE_ACTION,
  DELETE_CONFIRM_BODY,
  DELETE_CONFIRM_TITLE,
  EDIT,
  EMPTY_DESCRIPTION,
  EMPTY_TITLE,
  FETCH_BUNDLE,
  FETCH_UPDATE,
  HASH_PREVIEW_LENGTH,
  PATCH_ACTION,
  PUBLISH,
  UNPUBLISH,
} from "../library-copy";
import { rowActions, statusView } from "./catalog-status";
import EditFleetDialog from "./EditFleetDialog";

// Em dash, not an empty cell: a row with no bundle has a definite absence, and
// blank space reads as a rendering bug.
const NO_HASH = "—";

export default function PlatformCatalogTable({
  entries,
  onFetch,
}: {
  entries: PlatformCatalogEntry[];
  /** Opens the add/fetch dialog prefilled with this row's repository. */
  onFetch: (entry: PlatformCatalogEntry) => void;
}) {
  const [pending, startTransition] = useTransition();
  const [error, setError] = useState<string | null>(null);
  // Hold the row's ID, never the row object. Every write revalidates the page, so
  // a captured object goes stale the moment anything else lands — an operator would
  // then be editing a description read from a row the server has already replaced.
  const [editingId, setEditingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const editing = entries.find((e) => e.id === editingId) ?? null;
  const deleting = entries.find((e) => e.id === deletingId) ?? null;

  function setPublished(entry: PlatformCatalogEntry, published: boolean) {
    setError(null);
    startTransition(async () => {
      const result = await patchPlatformLibraryAction(entry.id, { published });
      if (!result.ok) {
        setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: PATCH_ACTION }));
      }
    });
  }

  async function confirmDelete(entry: PlatformCatalogEntry) {
    const result = await deletePlatformLibraryAction(entry.id);
    if (!result.ok) {
      setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: DELETE_ACTION }));
      return;
    }
    setDeletingId(null);
  }

  const columns: DataTableColumn<PlatformCatalogEntry>[] = [
    {
      key: "fleet",
      header: COLUMN_FLEET,
      cell: (row) => (
        <div className="flex flex-col">
          <span className="font-medium">{row.name}</span>
          <span className="text-xs text-muted-foreground">{row.id}</span>
        </div>
      ),
    },
    {
      key: "repository",
      header: COLUMN_REPOSITORY,
      hideOnMobile: true,
      cell: (row) => <span className="text-sm text-muted-foreground">{row.source_repo}</span>,
    },
    {
      key: "status",
      header: COLUMN_STATUS,
      cell: (row) => {
        const view = statusView(row);
        return (
          <Badge variant={view.tone} title={view.help}>
            {view.label}
          </Badge>
        );
      },
    },
    {
      key: "bundle",
      header: COLUMN_BUNDLE,
      hideOnMobile: true,
      // The hash is how an operator confirms a refetch actually changed something.
      cell: (row) => (
        <code className="text-xs text-muted-foreground">
          {row.content_hash ? row.content_hash.slice(0, HASH_PREVIEW_LENGTH) : NO_HASH}
        </code>
      ),
    },
    {
      key: "actions",
      header: COLUMN_ACTIONS,
      cell: (row) => {
        const actions = rowActions(row);
        return (
          <div className="flex items-center justify-end gap-1">
            {actions.canPublish ? (
              <IconAction
                label={PUBLISH}
                disabled={pending}
                onClick={() => setPublished(row, true)}
              >
                <EyeIcon size={14} />
              </IconAction>
            ) : null}
            {actions.canUnpublish ? (
              <IconAction
                label={UNPUBLISH}
                disabled={pending}
                onClick={() => setPublished(row, false)}
              >
                <EyeOffIcon size={14} />
              </IconAction>
            ) : null}
            <IconAction
              label={row.content_hash ? FETCH_UPDATE : FETCH_BUNDLE}
              disabled={pending}
              onClick={() => onFetch(row)}
            >
              <DownloadIcon size={14} />
            </IconAction>
            <IconAction label={EDIT} disabled={pending} onClick={() => setEditingId(row.id)}>
              <PencilIcon size={14} />
            </IconAction>
            {/* A published fleet has no Delete at all, rather than a disabled one:
                a disabled button is a promise. Withdraw it first. */}
            {actions.canDelete ? (
              <IconAction
                label={DELETE}
                disabled={pending}
                onClick={() => setDeletingId(row.id)}
              >
                <Trash2Icon size={14} />
              </IconAction>
            ) : null}
          </div>
        );
      },
    },
  ];

  return (
    <div className="space-y-4">
      {error ? (
        <p role="alert" data-testid="catalog-error" className="text-sm text-destructive">
          {error}
        </p>
      ) : null}

      <DataTable
        columns={columns}
        rows={entries}
        rowKey={(row) => row.id}
        caption={COLUMN_FLEET}
        empty={
          <EmptyState
            icon={<LibraryIcon size={20} aria-hidden="true" />}
            title={EMPTY_TITLE}
            description={EMPTY_DESCRIPTION}
          />
        }
      />

      {editing ? (
        <EditFleetDialog
          key={editing.id}
          entry={editing}
          open
          onOpenChange={() => setEditingId(null)}
        />
      ) : null}

      <ConfirmDialog
        open={deleting !== null}
        onOpenChange={() => setDeletingId(null)}
        title={DELETE_CONFIRM_TITLE}
        description={deleting ? `${deleting.name} — ${DELETE_CONFIRM_BODY}` : undefined}
        confirmLabel={DELETE}
        intent="destructive"
        onConfirm={deleting ? () => confirmDelete(deleting) : undefined}
      />
    </div>
  );
}
