"use client";

import { useMemo, useState, useTransition } from "react";
import {
  ConfirmDialog,
  DataTable,
  type DataTableColumn,
  Section,
  SectionHeader,
} from "@agentsfleet/design-system";
import type { Secret } from "@/lib/api/secrets";
import { presentErrorString } from "@/lib/errors";
import { requestOnboardingRefresh } from "@/lib/onboarding-refresh";
import type { TenantModelEntry, TenantModelEntryList, TenantPlatformDefault } from "@/lib/types";
import { listModelEntriesAction, listSecretsAction, resetProviderAction, setProviderSelfManagedAction, deleteModelEntryAction } from "../actions";
import { captureModelActivated, captureProviderReset } from "../lib/track";
import AddModelEntryDialog from "./AddModelEntryDialog";
import EditModelEntryDialog from "./EditModelEntryDialog";
import ModelDetailsDialog from "./ModelDetailsDialog";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import {
  ActionsCell,
  ContextCell,
  ModelCell,
  ProviderCell,
  type RegistryRow,
  StatusCell,
  rowKey,
} from "./ModelsRegistryCells";

type Props = { workspaceId: string; initial: TenantModelEntryList; initialSecrets: Secret[] };
export type SortState = { key: "model" | "provider"; dir: "ascending" | "descending" } | null;

// Pure — DataTable's onSortChange prop is typed `(key: string) => void` (any
// column could be sortable), but only the "model"/"provider" columns below
// opt in; a `key` outside that set returns `null` (no-op) instead of ever
// reaching component state. Exported so the boundary is unit-testable
// without needing to reach it through a real DataTable header click.
export function computeNextSort(cur: SortState, key: string): SortState | null {
  if (key !== "model" && key !== "provider") return null;
  if (!cur || cur.key !== key) return { key, dir: "ascending" };
  return { key, dir: cur.dir === "ascending" ? "descending" : "ascending" };
}

/** Pure — the sort comparator's per-row key, single call site per column. */
export function sortValueFor(entry: TenantModelEntry, key: "model" | "provider"): string {
  return key === "model" ? entry.model_id : (entry.provider ?? "");
}

const SWITCH_ACTION = "switch models";
const SWITCH_PLATFORM_ACTION = "switch to platform defaults";
const REMOVE_ACTION = "remove this model entry";

export default function ModelsRegistryTable({ workspaceId, initial, initialSecrets }: Props) {
  const [pending, startTransition] = useTransition();
  const [entries, setEntries] = useState<TenantModelEntry[]>(initial.models);
  const [secrets, setSecrets] = useState<Secret[]>(initialSecrets);
  const [platformDefaultAvailable, setPlatformDefaultAvailable] = useState(initial.platform_default_available);
  const [platformDefault, setPlatformDefault] = useState<TenantPlatformDefault | null>(initial.platform_default ?? null);
  const [sort, setSort] = useState<SortState>(null);
  const [error, setError] = useState<string | null>(null);
  const [detailsTarget, setDetailsTarget] = useState<TenantModelEntry | null>(null);
  const [editTarget, setEditTarget] = useState<TenantModelEntry | null>(null);
  const [removeTarget, setRemoveTarget] = useState<TenantModelEntry | null>(null);
  const [removeError, setRemoveError] = useState<string | null>(null);

  // The public model library — context + per-token rates for the Context
  // column's rates line. Already fetched once per session by the page-level
  // provider; a failed fetch degrades rates to "—", never the table.
  const { models: libraryModels } = useModelCatalogue();

  const hasActiveEntry = entries.some((e) => e.active);
  // The platform default is live only when it BOTH wins resolution (no active
  // tenant entry) and actually exists. Testing only the first half painted a
  // green "Active" badge on a default that was never configured — and because
  // ActionsCell short-circuits on a live default, it also suppressed the "No
  // default is configured" warning that would have said so. A fresh tenant on a
  // fresh install hits exactly that: core.model_library ships empty, so no
  // platform default can exist yet, and the first fleet run fails
  // PlatformKeyMissing while the UI reads healthy.
  const isDefaultLive = !hasActiveEntry && platformDefaultAvailable;
  // Hide the locked platform row outright when it is neither in effect nor
  // configurable-from-here: a self-managed tenant with no platform default was
  // shown a row it cannot act on and does not need.
  const showDefaultRow = !hasActiveEntry || platformDefaultAvailable;

  const sortedEntries = useMemo(() => {
    if (!sort) return entries;
    const dir = sort.dir === "ascending" ? 1 : -1;
    return [...entries].sort((a, b) => sortValueFor(a, sort.key).localeCompare(sortValueFor(b, sort.key)) * dir);
  }, [entries, sort]);

  const rows: RegistryRow[] = [
    ...(showDefaultRow ? [{ kind: "default" as const }] : []),
    ...sortedEntries.map((entry) => ({ kind: "entry" as const, entry })),
  ];

  function onSortChange(key: string) {
    const next = computeNextSort(sort, key);
    if (next) setSort(next);
  }

  function refresh() {
    startTransition(async () => {
      const r = await listModelEntriesAction();
      if (!r.ok) return;
      setEntries(r.data.models);
      setPlatformDefaultAvailable(r.data.platform_default_available);
      setPlatformDefault(r.data.platform_default ?? null);
    });
  }

  // Refetches only the secrets list — the cheaper counterpart to refresh()
  // above, called when AddModelEntryDialog commits a new stored secret.
  function refreshSecrets() {
    startTransition(async () => {
      const r = await listSecretsAction(workspaceId);
      if (!r.ok) return;
      setSecrets(r.data.secrets);
    });
  }

  // Only wired to the "Use default" button, which is disabled whenever
  // `!platformDefaultAvailable` — no redundant re-check needed here.
  function onSwitchDefault() {
    setError(null);
    const fromProvider = entries.find((e) => e.active)?.provider ?? "";
    startTransition(async () => {
      const r = await resetProviderAction();
      if (!r.ok) {
        // Failure Modes: "Stale activation" — a concurrent entry delete can
        // make this response stale, so refresh even on failure (matches
        // ApiKeyList's onConfirm — mirror backend reality regardless of outcome).
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: SWITCH_PLATFORM_ACTION }));
        refresh();
        return;
      }
      captureProviderReset(fromProvider);
      refresh();
    });
  }

  function onSwitchEntry(entry: TenantModelEntry) {
    setError(null);
    startTransition(async () => {
      const r = await setProviderSelfManagedAction({ secret_ref: entry.secret_ref, model: entry.model_id });
      if (!r.ok) {
        // Failure Modes: "Stale activation — Switch races a concurrent entry
        // delete; UI surfaces the existing friendly error and refreshes the list."
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: SWITCH_ACTION }));
        refresh();
        return;
      }
      captureModelActivated(r.data);
      requestOnboardingRefresh(workspaceId);
      refresh();
    });
  }

  // Bound to the active removeTarget when the confirm dialog is open (see
  // onConfirm below), so no in-function null check is needed — mirrors
  // ApiKeyList's RevokeConfirm.onConfirm(target: ConfirmTargetActive) shape.
  function confirmRemove(target: TenantModelEntry) {
    setRemoveError(null);
    startTransition(async () => {
      const r = await deleteModelEntryAction(target.id);
      if (!r.ok) {
        // Mirror backend reality regardless of outcome (ApiKeyList convention) —
        // a 409 active-entry guard can follow a concurrent Switch, so the table
        // behind the still-open confirm dialog reflects the current state.
        setRemoveError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: REMOVE_ACTION }));
        refresh();
        return;
      }
      setRemoveTarget(null);
      refresh();
    });
  }

  const columns: DataTableColumn<RegistryRow>[] = [
    { key: "provider", header: "Provider", sortable: true, cell: (row) => <ProviderCell row={row} platformDefault={platformDefault} /> },
    { key: "model", header: "Model", sortable: true, cell: (row) => <ModelCell row={row} platformDefault={platformDefault} /> },
    {
      key: "context",
      header: "Context · $/1M (in / cached / out)",
      numeric: true,
      hideOnMobile: true,
      cell: (row) => <ContextCell row={row} platformDefault={platformDefault} libraryModels={libraryModels} />,
    },
    { key: "status", header: "Status", cell: (row) => <StatusCell row={row} isDefaultLive={isDefaultLive} /> },
    {
      key: "actions",
      header: "Actions",
      numeric: true,
      cell: (row) => (
        <ActionsCell
          row={row}
          pending={pending}
          isDefaultLive={isDefaultLive}
          platformDefaultAvailable={platformDefaultAvailable}
          onSwitchDefault={onSwitchDefault}
          onSwitchEntry={onSwitchEntry}
          onView={setDetailsTarget}
          onEdit={setEditTarget}
          onRemove={setRemoveTarget}
        />
      ),
    },
  ];

  return (
    <Section asChild>
      <section aria-label="Models">
        <SectionHeader
          actions={
            <AddModelEntryDialog
              workspaceId={workspaceId}
              secrets={secrets}
              onCreated={refresh}
              onSecretsChanged={refreshSecrets}
            />
          }
        >
          Model registry
        </SectionHeader>

        <DataTable
          columns={columns}
          rows={rows}
          rowKey={rowKey}
          caption="Models"
          sortKey={sort?.key}
          sortDirection={sort?.dir}
          onSortChange={onSortChange}
        />

        {error ? <p className="text-sm text-destructive">{error}</p> : null}

        <ModelDetailsDialog target={detailsTarget} onOpenChange={() => setDetailsTarget(null)} />
        <EditModelEntryDialog
          workspaceId={workspaceId}
          target={editTarget}
          onOpenChange={() => setEditTarget(null)}
          onSaved={() => {
            setEditTarget(null);
            refresh();
          }}
          onPartialSuccess={refresh}
        />
        <ConfirmDialog
          open={removeTarget !== null}
          onOpenChange={() => {
            setRemoveTarget(null);
            setRemoveError(null);
          }}
          title={`Remove "${removeTarget?.model_id ?? ""}"?`}
          description="This removes the model entry only — the stored key and any sibling entry sharing it are untouched."
          confirmLabel="Remove"
          intent="destructive"
          errorMessage={removeError}
          onConfirm={removeTarget ? () => confirmRemove(removeTarget) : undefined}
        />
      </section>
    </Section>
  );
}
