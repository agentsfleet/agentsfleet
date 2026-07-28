"use client";

import { useMemo, useState, useTransition } from "react";
import {
  Button,
  ConfirmDialog,
  DataTable,
  type DataTableColumn,
  Section,
  SectionHeader,
} from "@agentsfleet/design-system";
import type { TenantModelEntryPageResult } from "@/lib/api/tenant_model_entries";
import { LIBRARY_ERROR_KIND, readErrorFrom, type LibraryError } from "@/lib/api/library-types";
import { presentErrorString } from "@/lib/errors";
import { requestOnboardingRefresh } from "@/lib/onboarding-refresh";
import type { TenantModelEntry, TenantPlatformDefault } from "@/lib/types";
import { listModelEntriesAction, resetProviderAction, setProviderSelfManagedAction, deleteModelEntryAction } from "../actions";
import { captureModelActivated, captureProviderReset } from "../lib/track";
import AddModelEntryDialog from "./AddModelEntryDialog";
import EditModelEntryDialog from "./EditModelEntryDialog";
import { computeNextSort, readErrorCopy, sortValueFor, type SortState } from "./registry-view";
import { useStoredSecrets } from "./use-stored-secrets";
import ModelDetailsDialog from "./ModelDetailsDialog";
import { CATALOGUE_STATUS } from "./catalogue-status";
import { maySpeculateOnHover, useModelCatalogue } from "./ModelCatalogueProvider";
import {
  ActionsCell,
  ContextCell,
  ModelCell,
  ProviderCell,
  type RegistryRow,
  StatusCell,
  rowKey,
} from "./ModelsRegistryCells";

type Props = {
  workspaceId: string;
  /** First registry page, or null when the read failed — see `initialError`. */
  initialPage: TenantModelEntryPageResult | null;
  /** Typed read failure. Distinct from an empty registry, and never both. */
  initialError: LibraryError | null;
};
const SWITCH_ACTION = "switch models";
const SWITCH_PLATFORM_ACTION = "switch to platform defaults";
const REMOVE_ACTION = "remove this model entry";

export default function ModelsRegistryTable({ workspaceId, initialPage, initialError }: Props) {
  const [pending, startTransition] = useTransition();
  const [entries, setEntries] = useState<TenantModelEntry[]>(initialPage?.models ?? []);
  // The secret list is NOT preloaded — the Add dialog loads it on open and
  // fails closed until it lands; the hook's docstring carries the why.
  const { secrets, secretsLoad, refreshSecrets } = useStoredSecrets(workspaceId);
  const [platformDefaultAvailable, setPlatformDefaultAvailable] = useState(
    initialPage?.platform_default_available ?? false,
  );
  const [platformDefault, setPlatformDefault] = useState<TenantPlatformDefault | null>(
    initialPage?.platform_default ?? null,
  );
  // Invariant 5: retained rows are only half the story — the cursor and total
  // say what has NOT been loaded, and both are rendered rather than implied.
  const [nextCursor, setNextCursor] = useState<string | null>(initialPage?.next_cursor ?? null);
  const [total, setTotal] = useState<number | null>(initialPage?.total ?? null);
  const [sort, setSort] = useState<SortState>(null);
  const [error, setError] = useState<string | null>(null);
  // The server read's typed failure. Held separately from `error` (which is
  // action feedback) so a failed LOAD never renders as an empty registry.
  const [readError, setReadError] = useState<LibraryError | null>(initialError);
  const [detailsTarget, setDetailsTarget] = useState<TenantModelEntry | null>(null);
  const [editTarget, setEditTarget] = useState<TenantModelEntry | null>(null);
  const [removeTarget, setRemoveTarget] = useState<TenantModelEntry | null>(null);
  const [removeError, setRemoveError] = useState<string | null>(null);

  // The public model library — a FALLBACK for the Context column's rates line,
  // used only where the server did not price a row (`identity.rate ?? …`). It
  // is no longer fetched on mount, so on an ordinary visit this is empty and
  // rows render their own server-provided rates. `preloadCatalogue` warms it on
  // intent to open a dialog whose picker genuinely needs it.
  const { models: libraryModels, status: catalogueStatus, preload: preloadCatalogue } = useModelCatalogue();

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

  // Re-read from the FIRST page, discarding retained rows only once a
  // replacement actually arrives. A failed refresh leaves what is on screen
  // alone and surfaces the fault — it must never fall back to empty.
  function refresh() {
    startTransition(async () => {
      // try/catch because the action ROUND-TRIP itself can reject (network
      // failure, deploy skew) — `withToken` only catches server-side. An
      // uncaught rejection here would escape the transition into a route
      // with no error boundary.
      try {
        const r = await listModelEntriesAction();
        if (!r.ok) {
          setReadError(readErrorFrom(r));
          return;
        }
        setReadError(null);
        setEntries(r.data.models);
        setPlatformDefaultAvailable(r.data.platform_default_available);
        setPlatformDefault(r.data.platform_default ?? null);
        setNextCursor(r.data.next_cursor);
        setTotal(r.data.total);
      } catch (cause) {
        setReadError({
          kind: LIBRARY_ERROR_KIND.unknown,
          detail: cause instanceof Error ? cause.message : undefined,
        });
      }
    });
  }

  // Append the next page, retaining every row already loaded. Exactly one
  // request per invocation — the walk this replaced issued as many as the
  // registry had pages, on every ordinary visit.
  //
  // Takes the cursor rather than reading `nextCursor` behind a null guard: the
  // control that calls this only renders when a next page exists, so the guard
  // could never fire and the type now carries that invariant instead.
  function loadMore(cursor: string) {
    startTransition(async () => {
      try {
        const r = await listModelEntriesAction(cursor);
        if (!r.ok) {
          setReadError(readErrorFrom(r));
          return;
        }
        setReadError(null);
        setEntries((prior) => [...prior, ...r.data.models]);
        // A cursor that does not advance means the server is re-serving the
        // same page; appending it forever would duplicate every row. The
        // exhaustive walk this replaced threw on exactly this defect —
        // treat it as terminal instead.
        setNextCursor(r.data.next_cursor === cursor ? null : r.data.next_cursor);
        setTotal(r.data.total);
      } catch (cause) {
        setReadError({
          kind: LIBRARY_ERROR_KIND.unknown,
          detail: cause instanceof Error ? cause.message : undefined,
        });
      }
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
          onEdit={(entry) => {
            // Opening is ungated intent — the picker inside needs the
            // catalogue now. Usually a no-op: focus or hover warmed it.
            preloadCatalogue();
            setEditTarget(entry);
          }}
          onEditFocusIntent={preloadCatalogue}
          onEditHoverIntent={() => {
            // A failed catalogue does not re-fetch on speculation: mousing
            // across rows against a failing backend would fire one request
            // per hover with no backoff. Deliberate intent (open) still
            // retries.
            if (catalogueStatus !== CATALOGUE_STATUS.error && maySpeculateOnHover()) preloadCatalogue();
          }}
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
              secretsLoad={secretsLoad}
              onCreated={refresh}
              onSecretsChanged={refreshSecrets}
              onSecretsNeeded={refreshSecrets}
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

        {/*
          Invariant 5. The walk this replaced guaranteed every row was present;
          paging cannot, so what is NOT loaded is stated rather than left to be
          inferred from whether a button happens to be rendered. With no total
          the remainder cannot be named, but its existence still is.
        */}
        {nextCursor !== null ? (
          <div className="flex items-center gap-3">
            <Button variant="secondary" onClick={() => loadMore(nextCursor)} disabled={pending}>
              {pending ? "Loading…" : "Load more"}
            </Button>
            <p className="text-sm text-muted-foreground" aria-live="polite">
              {total !== null
                ? `Showing ${entries.length} of ${total} models`
                : `Showing ${entries.length} models — more available`}
            </p>
          </div>
        ) : null}

        {/*
          A failed READ is not an empty registry. This renders alongside any
          rows already retained, so a refresh fault never blanks the table.
        */}
        {readError ? (
          <div role="alert" className="flex items-center gap-3">
            <p className="text-sm text-destructive">{readErrorCopy(readError)}</p>
            <Button variant="secondary" onClick={refresh} disabled={pending}>
              Retry
            </Button>
          </div>
        ) : null}

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
