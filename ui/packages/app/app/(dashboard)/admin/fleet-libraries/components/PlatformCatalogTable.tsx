"use client";

import type { PlatformEntry } from "@/lib/api/platform-library-types";
import { useState } from "react";
import { Alert, ConfirmDialog, DataTable, EmptyState } from "@agentsfleet/design-system";
import { LibraryIcon } from "lucide-react";
import EditFleetDialogDynamic from "@/components/domain/island-dynamic/EditFleetDialogDynamic";
import { DELETE, DELETE_CONFIRM_BODY, DELETE_CONFIRM_TITLE, EMPTY_DESCRIPTION, EMPTY_TITLE, FLEET_CATALOG_SECTION } from "../library-copy";
import { platformColumns } from "./platform-columns";
import { usePlatformCatalog } from "./use-platform-catalog";

export default function PlatformCatalogTable({entries}: {entries: PlatformEntry[]}) {
  const state = usePlatformCatalog(entries);
  // Dialogs retain the row the operator actually saw, including its revision tag.
  const [editing, setEditing] = useState<PlatformEntry | null>(null);
  const [deleting, setDeleting] = useState<PlatformEntry | null>(null);
  const columns = platformColumns({busy: state.busy, edit: setEditing, remove: (entry) => { state.clearError(); setDeleting(entry); },
    publish: (entry, published) => { void state.mutate(entry, published); }});
  return <div className="flex min-h-0 flex-col gap-md">
    {state.error ? <Alert variant="destructive" data-testid="catalog-error">{state.error}</Alert> : null}
    <DataTable columns={columns} rows={state.current} rowKey={(row) => row.id} caption={FLEET_CATALOG_SECTION} isLoading={state.busy}
      empty={<EmptyState icon={<LibraryIcon size={20} aria-hidden="true" />} title={EMPTY_TITLE} description={EMPTY_DESCRIPTION} />} />
    {editing ? <EditFleetDialogDynamic key={editing.id} entry={editing} open onOpenChange={() => setEditing(null)}
      onSaved={(updated) => { state.remember(editing.etag, updated); setEditing(null); }} /> : null}
    <ConfirmDialog open={deleting !== null} onOpenChange={() => setDeleting(null)}
      errorMessage={state.error}
      title={DELETE_CONFIRM_TITLE} description={deleting ? `${deleting.name} — ${DELETE_CONFIRM_BODY}` : undefined}
      confirmLabel={DELETE} intent="destructive" onConfirm={deleting ? async () => { if (await state.mutate(deleting)) setDeleting(null); } : undefined} />
  </div>;
}
