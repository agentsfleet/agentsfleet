"use client";

import { useMemo, useRef, useState, useTransition } from "react";
import {
  Button,
  ConfirmDialog,
  type DataTableColumn,
  Section,
  SectionHeader,
} from "@agentsfleet/design-system";
import type { TenantModelEntryPageResult } from "@/lib/api/tenant_model_entries";
import { LIBRARY_ERROR_KIND, readErrorFrom, type LibraryError } from "@/lib/api/library-types";
import { presentErrorString } from "@/lib/errors";
import { maySpeculateOnHover } from "@/components/domain/island-dynamic/intent-module-loader";
import type { TenantModelEntry, TenantPlatformDefault } from "@/lib/types";
import { listModelEntriesAction, resetProviderAction, setProviderSelfManagedAction, deleteModelEntryAction } from "../actions";
import { captureModelActivated, captureProviderReset } from "../lib/track";
import AddModelEntryDialog from "./AddModelEntryDialog";
import EditModelEntryDialog from "./EditModelEntryDialog";
import { compareEntries, computeNextSort, readErrorCopy, type SortState } from "./registry-view";
import { useStoredSecrets } from "./use-stored-secrets";
import ModelDetailsDialog from "./ModelDetailsDialog";
import { CATALOGUE_STATUS } from "./catalogue-status";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import {
  ActionsCell,
  ContextCell,
  RatesCell,
  ModelCell,
  ProviderCell,
  type RegistryRow,
  StatusCell,
} from "./ModelsRegistryCells";
import { ModelsRegistryView } from "./ModelsRegistryView";
import { CONTEXT_HEADER, RATES_HEADER } from "@/lib/models/display";

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
  const readGeneration = useRef(0);
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

  // The catalogue warms on dialog intent; rows already carry their own rates.
  const { models: libraryModels, status: catalogueStatus, preload: preloadCatalogue } = useModelCatalogue();

  const hasActiveEntry = entries.some((e) => e.active);
  // An absent default must never be presented as active.
  const isDefaultLive = !hasActiveEntry && platformDefaultAvailable;
  // Hide the locked platform row outright when it is neither in effect nor
  // configurable-from-here: a self-managed tenant with no platform default was
  // shown a row it cannot act on and does not need.
  const showDefaultRow = !hasActiveEntry || platformDefaultAvailable;

  const sortedEntries = useMemo(() => {
    if (!sort) return entries;
    const dir = sort.dir === "ascending" ? 1 : -1;
    return [...entries].sort((a, b) => compareEntries(a, b, sort.key, libraryModels) * dir);
  }, [entries, sort, libraryModels]);

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
    const generation = ++readGeneration.current;
    startTransition(async () => {
      // try/catch because the action ROUND-TRIP itself can reject (network
      // failure, deploy skew) — `withToken` only catches server-side. An
      // uncaught rejection here would escape the transition into a route
      // with no error boundary.
      try {
        const r = await listModelEntriesAction();
        if (generation !== readGeneration.current) return;
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
        if (generation !== readGeneration.current) return;
        setReadError({
          kind: LIBRARY_ERROR_KIND.unknown,
          detail: cause instanceof Error ? cause.message : undefined,
        });
      }
    });
  }

  function loadMore(cursor: string): Promise<boolean> {
    const generation = readGeneration.current;
    return new Promise((resolve) => startTransition(async () => {
      try {
        const result = await listModelEntriesAction(cursor);
        if (generation !== readGeneration.current) { resolve(false); return; }
        if (!result.ok) { setReadError(readErrorFrom(result)); resolve(false); return; }
        setReadError(null);
        setEntries((prior) => [...new Map([...prior, ...result.data.models].map((entry) => [entry.id, entry])).values()]);
        setNextCursor(result.data.next_cursor === cursor ? null : result.data.next_cursor);
        setTotal(result.data.total);
        resolve(true);
      } catch {
        if (generation === readGeneration.current) setReadError({kind: LIBRARY_ERROR_KIND.unknown});
        resolve(false);
      }
    }));
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
    { key: "model", primary: true, header: "Model", sortable: true, cell: (row) => <ModelCell row={row} platformDefault={platformDefault} /> },
    {
      key: "context",
      header: CONTEXT_HEADER,
      numeric: true, sortable: true,
      hideOnMobile: true,
      cell: (row) => <ContextCell row={row} platformDefault={platformDefault} libraryModels={libraryModels} />,
    },
    {
      key: "rates",
      header: RATES_HEADER,
      numeric: true, sortable: true,
      hideOnMobile: true,
      cell: (row) => <RatesCell row={row} platformDefault={platformDefault} libraryModels={libraryModels} />,
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
          onView={(entry) => { preloadCatalogue(); setDetailsTarget(entry); }}
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
        <SectionHeader as="p"
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
          Available models
        </SectionHeader>

        <ModelsRegistryView columns={columns} rows={rows} platformDefault={platformDefault} sort={sort}
          onSortChange={onSortChange} pending={pending}
          onLoadMore={nextCursor ? () => loadMore(nextCursor) : undefined} />

        {nextCursor !== null ? <p className="text-body-sm text-muted-foreground" aria-live="polite">
          {total !== null ? `Showing ${entries.length} of ${total} models` : `Showing ${entries.length} models — more available`}
        </p> : null}

        {/*
          A failed READ is not an empty registry. This renders alongside any
          rows already retained, so a refresh fault never blanks the table.
        */}
        {readError ? (
          <div role="alert" className="flex items-center gap-3">
            <p className="text-sm text-destructive">{readErrorCopy(readError)}</p>
            <Button variant="ghost" onClick={refresh} disabled={pending}>
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
          onCommitted={refresh}
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
