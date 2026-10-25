"use client";

import type { PlatformEntry } from "@/lib/api/platform-library-types";
import { useState } from "react";
import { Alert, ConfirmDialog, DataTable } from "@agentsfleet/design-system";
import { FleetLibraryEmptyState } from "@/components/domain/FleetLibraryEmptyState";
import EditFleetDialogDynamic from "@/components/domain/island-dynamic/EditFleetDialogDynamic";
import { DELETE, DELETE_CONFIRM_BODY, DELETE_CONFIRM_TITLE, EMPTY_DESCRIPTION, FLEET_SETUPS_SECTION } from "../library-copy";
import { platformColumns } from "./platform-columns";
import { usePlatformCatalog } from "./use-platform-catalog";

export default function PlatformCatalogTable({entries}: {entries: PlatformEntry[]}) {
  const state = usePlatformCatalog(entries);
  // Dialogs retain the row the operator actually saw, including its revision tag.
  const [editing, setEditing] = useState<{entry: PlatformEntry; opener: HTMLElement} | null>(null);
  const [deleting, setDeleting] = useState<PlatformEntry | null>(null);
  const columns = platformColumns({busy: state.busy, edit: (entry, opener) => setEditing({entry, opener}), remove: (entry) => { state.clearError(); setDeleting(entry); },
    publish: (entry, published) => { void state.mutate(entry, published); }});
  return <div className="flex min-h-0 flex-col gap-md">
    {state.error ? <Alert variant="destructive" data-testid="catalog-error">{state.error}</Alert> : null}
    <DataTable columns={columns} rows={state.current} rowKey={(row) => row.id} caption={FLEET_SETUPS_SECTION} isLoading={state.busy}
      empty={<FleetLibraryEmptyState createHref="/" description={EMPTY_DESCRIPTION} />} />
    {editing ? <EditFleetDialogDynamic key={editing.entry.id} entry={editing.entry} returnFocusTo={editing.opener} open onOpenChange={() => setEditing(null)}
      onSaved={(updated) => { state.remember(editing.entry.etag, updated); setEditing(null); }} /> : null}
    <ConfirmDialog open={deleting !== null} onOpenChange={() => setDeleting(null)}
      errorMessage={state.error}
      title={DELETE_CONFIRM_TITLE} description={deleting ? `${deleting.name} — ${DELETE_CONFIRM_BODY}` : undefined}
      confirmLabel={DELETE} intent="destructive" onConfirm={deleting ? async () => { if (await state.mutate(deleting)) setDeleting(null); } : undefined} />
  </div>;
}
