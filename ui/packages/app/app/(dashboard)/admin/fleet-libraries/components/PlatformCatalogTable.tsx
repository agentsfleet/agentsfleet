"use client";

import { useState } from "react";
import {
  Badge,
  Button,
  ConfirmDialog,
  CopyButton,
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
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
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
  COPY_HASH_LABEL,
  COPY_SLUG_LABEL,
  HASH_PREVIEW_LENGTH,
  PATCH_ACTION,
  PUBLISH,
  REPOSITORY_HOST,
  REPOSITORY_LINK_LABEL,
  SOURCE_REF_PATTERN,
  UNPUBLISH,
} from "../library-copy";
import { rowActions, statusView } from "./catalog-status";
import EditFleetDialog from "./EditFleetDialog";

// Em dash, not an empty cell: a row with no bundle has a definite absence, and
// blank space reads as a rendering bug.
const NO_HASH = "—";

// A platform row is normally imported from GitHub, so its source is `owner/repo`
// and an operator wants to click through and check it. A template- or
// upload-sourced row is not, and linking it would point at a repository that does
// not exist — so the cell only becomes a link when the value is actually a slug.
function RepositoryCell({ repo }: { repo: string }) {
  if (!SOURCE_REF_PATTERN.test(repo)) {
    return <span className="text-sm text-muted-foreground">{repo}</span>;
  }
  return (
    <Button asChild variant="link" size="sm" className="h-auto p-0 text-sm font-normal">
      <a
        href={`${REPOSITORY_HOST}${repo}`}
        target="_blank"
        rel="noopener noreferrer"
        aria-label={`${REPOSITORY_LINK_LABEL}: ${repo}`}
      >
        {repo}
      </a>
    </Button>
  );
}

const ACTION_PUBLISHED = "published";
const ACTION_UNPUBLISHED = "unpublished";
const OUTCOME_SUCCESS = "success";
const OUTCOME_FAILURE = "failure";

type EntryOverride = {
  baseEtag: string;
  entry: PlatformCatalogEntry;
};

export default function PlatformCatalogTable({
  entries,
  onFetch,
}: {
  entries: PlatformCatalogEntry[];
  /** Opens the add/fetch dialog prefilled with this row's repository. */
  onFetch: (entry: PlatformCatalogEntry) => void;
}) {
  const [patchPending, setPatchPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Hold the row's ID, never the row object. Every write revalidates the page, so
  // a captured object goes stale the moment anything else lands — an operator would
  // then be editing a description read from a row the server has already replaced.
  const [editingId, setEditingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [deletePending, setDeleting] = useState(false);
  const [entryOverrides, setEntryOverrides] = useState<Record<string, EntryOverride>>({});

  const currentEntries = entries.map((entry) => {
    const override = entryOverrides[entry.id];
    return override?.baseEtag === entry.etag ? override.entry : entry;
  });

  const editing = currentEntries.find((e) => e.id === editingId) ?? null;
  const deleting = currentEntries.find((e) => e.id === deletingId) ?? null;

  // One signal for "a write is in flight", covering publish/unpublish AND delete.
  const busy = patchPending || deletePending;

  async function setPublished(entry: PlatformCatalogEntry, published: boolean) {
    setError(null);
    setPatchPending(true);
    try {
      const result = await patchPlatformLibraryAction(entry.id, { published }, entry.etag);
      // Publishing is the moment a fleet becomes available to every tenant. A
      // refusal is recorded too — a publish nobody could complete is a signal, not
      // an absence of one.
      captureProductEvent(EVENTS.platform_library_published, {
        entry_id: entry.id,
        action: published ? ACTION_PUBLISHED : ACTION_UNPUBLISHED,
        outcome: result.ok ? OUTCOME_SUCCESS : OUTCOME_FAILURE,
      });
      if (!result.ok) {
        setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: PATCH_ACTION }));
        return;
      }
      rememberServerEntry(entry.etag, result.data);
    } finally {
      setPatchPending(false);
    }
  }

  function rememberServerEntry(baseEtag: string, updated: PlatformCatalogEntry) {
    // The action started with this server-component ETag; keep that baseline
    // until revalidation supplies a newer row, even if another write races.
    setEntryOverrides((current) => {
      const baseline = current[updated.id]?.baseEtag ?? baseEtag;
      return {
        ...current,
        [updated.id]: { baseEtag: baseline, entry: updated },
      };
    });
  }

  // ConfirmDialog owns its own pending state and disables both of its buttons while
  // this resolves, so the confirm cannot be double-fired. What it does NOT do is tell
  // the TABLE that a write is in flight — so without this flag an operator could
  // publish or refetch a row whose delete is still running. Every row action is gated
  // on one pending signal, and delete is not an exception to it.
  async function confirmDelete(entry: PlatformCatalogEntry) {
    setDeleting(true);
    try {
      const result = await deletePlatformLibraryAction(entry.id);
      if (!result.ok) {
        setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: DELETE_ACTION }));
        return;
      }
      setDeletingId(null);
    } finally {
      setDeleting(false);
    }
  }

  const columns: DataTableColumn<PlatformCatalogEntry>[] = [
    {
      key: "fleet",
      header: COLUMN_FLEET,
      cell: (row) => (
        <div className="flex flex-col">
          <span className="font-medium">{row.name}</span>
          {/* The slug is the id a workspace installs by (`platform_library_id`) and
              the id every API call names. It is meant to be pasted, so it can be. */}
          <span className="flex items-center gap-1">
            <span className="text-xs text-muted-foreground">{row.id}</span>
            <CopyButton value={row.id} label={`${COPY_SLUG_LABEL}: ${row.id}`} />
          </span>
        </div>
      ),
    },
    {
      key: "repository",
      header: COLUMN_REPOSITORY,
      hideOnMobile: true,
      cell: (row) => <RepositoryCell repo={row.source_repo} />,
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
      // The hash is how an operator confirms a refetch actually changed something —
      // comparing two of them IS the job this column exists for. The cell shows a
      // preview (the full hash would dominate the row) and copies the WHOLE hash,
      // because a truncated one compares to nothing.
      cell: (row) =>
        row.content_hash ? (
          <span className="flex items-center gap-1">
            <code className="text-xs text-muted-foreground">
              {row.content_hash.slice(0, HASH_PREVIEW_LENGTH)}
            </code>
            <CopyButton value={row.content_hash} label={COPY_HASH_LABEL} />
          </span>
        ) : (
          <code className="text-xs text-muted-foreground">{NO_HASH}</code>
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
                disabled={busy}
                onClick={() => void setPublished(row, true)}
              >
                <EyeIcon size={14} />
              </IconAction>
            ) : null}
            {actions.canUnpublish ? (
              <IconAction
                label={UNPUBLISH}
                disabled={busy}
                onClick={() => void setPublished(row, false)}
              >
                <EyeOffIcon size={14} />
              </IconAction>
            ) : null}
            <IconAction
              label={row.content_hash ? FETCH_UPDATE : FETCH_BUNDLE}
              disabled={busy}
              onClick={() => onFetch(row)}
            >
              <DownloadIcon size={14} />
            </IconAction>
            <IconAction label={EDIT} disabled={busy} onClick={() => setEditingId(row.id)}>
              <PencilIcon size={14} />
            </IconAction>
            {/* A published fleet has no Delete at all, rather than a disabled one:
                a disabled button is a promise. Withdraw it first. */}
            {actions.canDelete ? (
              <IconAction
                label={DELETE}
                disabled={busy}
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
        rows={currentEntries}
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
          onSaved={(updated) => {
            rememberServerEntry(editing.etag, updated);
            setEditingId(null);
          }}
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
