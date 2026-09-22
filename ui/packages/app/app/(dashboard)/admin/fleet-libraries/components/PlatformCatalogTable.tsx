"use client";

import { useState } from "react";
import {
  Badge,
  ConfirmDialog,
  CopyButton,
  DataTable,
  type DataTableColumn,
  EmptyState,
  IconAction,
  Time,
} from "@agentsfleet/design-system";
import {
  DownloadIcon,
  EyeIcon,
  EyeOffIcon,
  LibraryIcon,
  PencilIcon,
  Trash2Icon,
} from "lucide-react";
import {
  SOURCE_KIND_GITHUB,
  SOURCE_KIND_UPLOAD,
  SourceMark,
} from "@/components/domain/fleet-library/SourceMark";
import type { PlatformCatalogEntry } from "@/lib/types";
import { presentErrorString } from "@/lib/errors";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { preloadAddFleetDialog } from "@/components/domain/island-dynamic/AddFleetDialogDynamic";
import {
  default as EditFleetDialogDynamic,
  preloadEditFleetDialog,
} from "@/components/domain/island-dynamic/EditFleetDialogDynamic";
import {
  maySpeculateOnHover,
} from "@/components/domain/island-dynamic/intent-module-loader";
import { deletePlatformLibraryAction, patchPlatformLibraryAction } from "../actions";
import {
  COLUMN_ACTIONS,
  COLUMN_BUNDLE,
  COLUMN_NAME,
  COLUMN_SOURCE,
  COLUMN_STATUS,
  COLUMN_TIME,
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
  FLEET_CATALOG_SECTION,
  HASH_PREVIEW_LENGTH,
  PATCH_ACTION,
  PUBLISH,
  SOURCE_REF_PATTERN,
  UNPUBLISH,
} from "../library-copy";
import { rowActions, statusView } from "./catalog-status";

// Em dash, not an empty cell: a row with no bundle has a definite absence, and
// blank space reads as a rendering bug.
const NO_HASH = "—";

// This table draws its source exactly as the workspace Fleet library draws its
// own, through one component, so the two surfaces cannot drift apart a glyph at
// a time.
//
// The kind is derived rather than read: `core.fleet_library` stores no
// `source_kind` (schema/450_fleet_library.sql), and an upload leaves
// `source_repo` empty — the same predicate `rowActions` keys Fetch off, so the
// glyph and the affordance can never disagree about what a row is.
function sourceKindOf(entry: PlatformCatalogEntry): string {
  return SOURCE_REF_PATTERN.test(entry.source_repo) ? SOURCE_KIND_GITHUB : SOURCE_KIND_UPLOAD;
}

// Unlike the workspace row, a platform row DOES store the revision it was
// fetched at, so the link is pinned to it. `source_ref` takes a branch, a tag,
// or a commit (afd_library::github), and only the last of the three actually
// holds still — a link to `main` is a link to whatever main is today, which is
// what the row's title says.
function sourceCell(entry: PlatformCatalogEntry) {
  return (
    <SourceMark
      kind={sourceKindOf(entry)}
      sourceRef={entry.source_repo}
      gitRef={entry.source_ref || undefined}
    />
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
      key: "name",
      header: COLUMN_NAME,
      sortValue: (row) => row.name,
      // The name alone. The slug (`platform_library_id`) used to ride under it
      // with a copy button; it is an API identifier, and the Edit dialog is
      // where a row is worked on, so the table reads as a catalogue instead of
      // a clipboard.
      cell: (row) => <span className="font-medium">{row.name}</span>,
    },
    {
      key: "source",
      header: COLUMN_SOURCE,
      hideOnMobile: true,
      sortValue: (row) => `${sourceKindOf(row)}:${row.source_repo}`,
      cell: sourceCell,
    },
    {
      key: "status",
      header: COLUMN_STATUS,
      sortValue: (row) => statusView(row).label,
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
      sortValue: (row) => row.content_hash ?? "",
      // The hash is how an operator confirms a refetch actually changed something —
      // comparing two of them IS the job this column exists for. The cell shows a
      // preview (the full hash would dominate the row) and copies the WHOLE hash,
      // because a truncated one compares to nothing.
      cell: (row) =>
        row.content_hash ? (
          <span className="flex items-center gap-1">
            <code className="text-mono leading-mono text-muted-foreground">
              {row.content_hash.slice(0, HASH_PREVIEW_LENGTH)}
            </code>
            <CopyButton value={row.content_hash} label={COPY_HASH_LABEL} />
          </span>
        ) : (
          <code className="text-mono leading-mono text-muted-foreground">{NO_HASH}</code>
        ),
    },
    {
      key: "updated_at",
      header: COLUMN_TIME,
      hideOnMobile: true,
      sortValue: (row) => row.updated_at,
      // `updated_at`, not a creation instant: the catalogue carries no other,
      // and on this table the question is "when did this row last move" — a
      // refetch, a publish, an edit — which is the one it answers.
      cell: (row) => (
        <Time value={new Date(row.updated_at)} format="relative" className="tabular-nums" />
      ),
    },
    {
      key: "actions",
      header: COLUMN_ACTIONS,
      numeric: true,
      cell: (row) => {
        const actions = rowActions(row);
        return (
          <div className="flex items-center justify-end gap-1">
            <IconAction
              variant="ghost"
              label={EDIT}
              disabled={busy}
              onFocus={preloadEditFleetDialog}
              onPointerEnter={() => {
                if (maySpeculateOnHover()) preloadEditFleetDialog();
              }}
              onClick={() => setEditingId(row.id)}
            >
              <PencilIcon size={14} />
            </IconAction>
            {actions.canPublish ? (
              <IconAction
                variant="ghost"
                label={PUBLISH}
                disabled={busy}
                onClick={() => void setPublished(row, true)}
              >
                <EyeIcon size={14} />
              </IconAction>
            ) : null}
            {actions.canUnpublish ? (
              <IconAction
                variant="ghost"
                label={UNPUBLISH}
                disabled={busy}
                onClick={() => void setPublished(row, false)}
              >
                <EyeOffIcon size={14} />
              </IconAction>
            ) : null}
            {/* An uploaded row has no repository behind it, so there is nothing to
                re-read; the affordance is absent rather than disabled, same as
                Delete on a published row. */}
            {actions.canFetch ? (
              <IconAction
                variant="ghost"
                label={row.content_hash ? FETCH_UPDATE : FETCH_BUNDLE}
                disabled={busy}
                onFocus={preloadAddFleetDialog}
                onPointerEnter={() => {
                  if (maySpeculateOnHover()) preloadAddFleetDialog();
                }}
                onClick={() => onFetch(row)}
              >
                <DownloadIcon size={14} />
              </IconAction>
            ) : null}
            {/* A published fleet has no Delete at all, rather than a disabled one:
                a disabled button is a promise. Withdraw it first. */}
            {actions.canDelete ? (
              <IconAction
                variant="destructive"
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
    // Fills the height its page already reserves. `FleetLibrariesView` wraps
    // this in `h-full overflow-hidden` + `flex min-h-0 flex-1 flex-col`, but
    // the table stopped at its own content, so a one-row catalog floated in
    // the top of the screen with its pagination bar tucked beneath it while
    // the workspace Fleet library — same data, same DataTable — spanned. The
    // shell was never the difference; this wrapper was.
    <div className="flex min-h-0 flex-1 flex-col gap-4">
      {error ? (
        <p role="alert" data-testid="catalog-error" className="text-sm text-destructive">
          {error}
        </p>
      ) : null}

      <DataTable
        className="flex min-h-0 flex-1 flex-col"
        viewportClassName="min-h-0 flex-1 max-h-none"
        columns={columns}
        rows={currentEntries}
        rowKey={(row) => row.id}
        caption={FLEET_CATALOG_SECTION}
        empty={
          <EmptyState
            icon={<LibraryIcon size={20} aria-hidden="true" />}
            title={EMPTY_TITLE}
            description={EMPTY_DESCRIPTION}
          />
        }
      />

      {editing ? (
        <EditFleetDialogDynamic
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
