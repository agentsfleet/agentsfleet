"use client";

import { useMemo, useRef, useState, useTransition } from "react";
import {
  Button,
  ConfirmDialog,
  Section,
  PageHeader,
  PageTitle,
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
import { readErrorCopy } from "./registry-view";
import { useStoredSecrets } from "./use-stored-secrets";
import ModelDetailsDialog from "./ModelDetailsDialog";
import { CATALOGUE_STATUS } from "./catalogue-status";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import { ModelsRegistryView } from "./ModelsRegistryView";
import { MODELS_PAGE_DESCRIPTION } from "../copy";
import { useCurrentUser } from "@/lib/auth/client";

type Props = {
  workspaceId: string;
  workspaces?: ReadonlyArray<{id: string; name: string | null}>;
  /** First registry page, or null when the read failed — see `initialError`. */
  initialPage: TenantModelEntryPageResult | null;
  /** Typed read failure. Distinct from an empty registry, and never both. */
  initialError: LibraryError | null;
};
const SWITCH_ACTION = "switch models";
const SWITCH_PLATFORM_ACTION = "switch to platform defaults";
const REMOVE_ACTION = "remove this model entry";

export default function ModelsRegistryTable({ workspaceId, workspaces, initialPage, initialError }: Props) {
  const {userId} = useCurrentUser();
  const workspaceNames = useMemo(() => new Map(workspaces?.map(({id, name}) => [id, name ?? "Unnamed workspace"])), [workspaces]);
  const [pending, startTransition] = useTransition();
  const readGeneration = useRef(0);
  const [entries, setEntries] = useState<TenantModelEntry[]>(initialPage?.models ?? []);
  // The secret list is NOT preloaded — the Add dialog loads it on open and
  // fails closed until it lands; the hook's docstring carries the why.
  const { secrets, secretsLoad, refreshSecrets } = useStoredSecrets(workspaceId, userId);
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
  const [error, setError] = useState<string | null>(null);
  // The server read's typed failure. Held separately from `error` (which is
  // action feedback) so a failed LOAD never renders as an empty registry.
  const [readError, setReadError] = useState<LibraryError | null>(initialError);
  const [detailsTarget, setDetailsTarget] = useState<TenantModelEntry | null>(null);
  const [editTarget, setEditTarget] = useState<TenantModelEntry | null>(null);
  const [removeTarget, setRemoveTarget] = useState<TenantModelEntry | null>(null);
  const [removeError, setRemoveError] = useState<string | null>(null);

  // The catalogue warms on dialog intent; rows already carry their own rates.
  const { status: catalogueStatus, preload: preloadCatalogue } = useModelCatalogue();

  const hasActiveEntry = entries.some((e) => e.active);
  // An absent default must never be presented as active.
  const isDefaultLive = nextCursor === null && !hasActiveEntry && platformDefaultAvailable;
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

  function onSwitchEntry(entry: TenantModelEntry, sourceWorkspace: string) {
    setError(null);
    startTransition(async () => {
      const r = await setProviderSelfManagedAction({ workspace_id: sourceWorkspace, secret_ref: entry.secret_ref, model: entry.model_id });
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

  return (
    <Section asChild className="gap-2xl">
      <section aria-label="Models">
        <PageHeader className="sm:flex-col summary-medium:flex-row" description={MODELS_PAGE_DESCRIPTION}
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
          <PageTitle>Models</PageTitle>
        </PageHeader>

        {readError && entries.length === 0 ? null : <ModelsRegistryView entries={entries} platformDefault={platformDefault} workspaceNames={workspaceNames}
          isDefaultLive={isDefaultLive} platformDefaultAvailable={platformDefaultAvailable} pending={pending}
          onSwitchDefault={onSwitchDefault} onSwitchEntry={onSwitchEntry}
          onView={entry => {preloadCatalogue(); setDetailsTarget(entry);}}
          onEdit={entry => {preloadCatalogue(); setEditTarget(entry);}} onRemove={setRemoveTarget}
          onIntent={() => {if (catalogueStatus !== CATALOGUE_STATUS.error && maySpeculateOnHover()) preloadCatalogue();}}
          footerAction={<AddModelEntryDialog workspaceId={workspaceId} secrets={secrets} secretsLoad={secretsLoad} onCreated={refresh} onSecretsChanged={refreshSecrets} onSecretsNeeded={refreshSecrets} triggerLabel="Add another model" secondary />}
          onLoadMore={nextCursor ? () => loadMore(nextCursor) : undefined} />}

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

        <ModelDetailsDialog target={detailsTarget} workspaceLabel={workspaceNames.get(detailsTarget?.workspace_id ?? "")}
          onOpenChange={() => setDetailsTarget(null)} />
        <EditModelEntryDialog
          workspaceId={editTarget?.workspace_id ?? ""}
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
          title={`Remove “${removeTarget?.model_id ?? ""}”?`}
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
