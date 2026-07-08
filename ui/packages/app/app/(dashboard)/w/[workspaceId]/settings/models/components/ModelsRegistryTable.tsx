"use client";

import { useMemo, useState, useTransition } from "react";
import {
  Badge,
  Button,
  ConfirmDialog,
  DataTable,
  type DataTableColumn,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Section,
  SectionLabel,
} from "@agentsfleet/design-system";
import { ArrowLeftRightIcon, LockIcon, MoreHorizontalIcon } from "lucide-react";
import type { Secret } from "@/lib/api/secrets";
import { providerLabel } from "@/lib/api/model_caps";
import { presentErrorString } from "@/lib/errors";
import type { TenantModelEntry, TenantModelEntryList } from "@/lib/types";
import { listModelEntriesAction, listSecretsAction, resetProviderAction, setProviderSelfManagedAction, deleteModelEntryAction } from "../actions";
import { captureModelActivated, captureProviderReset } from "../lib/track";
import AddModelEntryDialog from "./AddModelEntryDialog";
import EditModelEntryDialog from "./EditModelEntryDialog";
import ModelDetailsDialog from "./ModelDetailsDialog";

type Props = { workspaceId: string; initial: TenantModelEntryList; initialSecrets: Secret[] };
type RegistryRow = { kind: "default" } | { kind: "entry"; entry: TenantModelEntry };
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
const PLATFORM_UNAVAILABLE_NOTE = "No default is configured.";
// Threshold + divisor for the "k" context abbreviation (200000 → "200k").
const TOKENS_PER_K = 1000;

// `context_cap_tokens` is a Zig `?u32` on the wire (schema/embed.zig) — the
// only real-world absent case is "not in the catalogue" (undefined). Guard
// on nullishness, not falsiness, so a (semantically invalid but not
// impossible) explicit 0 still renders as "0" rather than "—".
function formatContext(tokens: number | undefined): string {
  if (tokens == null) return "—";
  return tokens >= TOKENS_PER_K ? `${Math.round(tokens / TOKENS_PER_K)}k` : String(tokens);
}

function rowKey(row: RegistryRow): string {
  return row.kind === "default" ? "default" : row.entry.id;
}

function ModelCell({ row }: { row: RegistryRow }) {
  if (row.kind === "default") {
    return (
      <span className="inline-flex items-center gap-2">
        <span>Default</span>
        <LockIcon size={12} className="text-muted-foreground" aria-label="Managed by a platform admin" />
      </span>
    );
  }
  return <span className="truncate font-mono text-sm">{row.entry.model_id}</span>;
}

function ProviderCell({ row }: { row: RegistryRow }) {
  if (row.kind === "default") return <span className="text-xs text-muted-foreground">Platform-managed</span>;
  const { entry } = row;
  return (
    <div className="min-w-0">
      <div className="text-sm">{entry.provider ? providerLabel(entry.provider) : "Unknown"}</div>
      {entry.base_url ? <div className="truncate font-mono text-xs text-muted-foreground">{entry.base_url}</div> : null}
    </div>
  );
}

function StatusCell({ row, isDefaultLive }: { row: RegistryRow; isDefaultLive: boolean }) {
  if (row.kind === "default") return isDefaultLive ? <Badge variant="green">Active</Badge> : null;
  const { entry } = row;
  if (entry.active) return <Badge variant="green">Active</Badge>;
  if (!entry.has_key) return <Badge variant="default">no key · local</Badge>;
  return null;
}

function RowMenu({
  entry,
  pending,
  onView,
  onEdit,
  onRemove,
}: {
  entry: TenantModelEntry;
  pending: boolean;
  onView: (e: TenantModelEntry) => void;
  onEdit: (e: TenantModelEntry) => void;
  onRemove: (e: TenantModelEntry) => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger aria-label={`Row actions for ${entry.model_id}`} disabled={pending}>
        <MoreHorizontalIcon size={14} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onSelect={() => onView(entry)}>View details</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => onEdit(entry)}>Edit</DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          onSelect={() => onRemove(entry)}
          disabled={entry.active}
          aria-label={entry.active ? `Cannot remove ${entry.model_id} while it is active` : `Remove ${entry.model_id}`}
        >
          Remove
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function ActionsCell({
  row,
  pending,
  isDefaultLive,
  platformDefaultAvailable,
  onSwitchDefault,
  onSwitchEntry,
  onView,
  onEdit,
  onRemove,
}: {
  row: RegistryRow;
  pending: boolean;
  isDefaultLive: boolean;
  platformDefaultAvailable: boolean;
  onSwitchDefault: () => void;
  onSwitchEntry: (e: TenantModelEntry) => void;
  onView: (e: TenantModelEntry) => void;
  onEdit: (e: TenantModelEntry) => void;
  onRemove: (e: TenantModelEntry) => void;
}) {
  if (row.kind === "default") {
    if (isDefaultLive) return null;
    return (
      <div className="flex flex-col items-end gap-1">
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={pending || !platformDefaultAvailable}
          onClick={onSwitchDefault}
          className="gap-1.5"
        >
          <ArrowLeftRightIcon size={14} />
          Use default
        </Button>
        {!platformDefaultAvailable ? <span className="text-xs text-muted-foreground">{PLATFORM_UNAVAILABLE_NOTE}</span> : null}
      </div>
    );
  }
  const { entry } = row;
  return (
    <div className="flex items-center justify-end gap-2">
      {!entry.active ? (
        <Button type="button" size="sm" disabled={pending} onClick={() => onSwitchEntry(entry)} className="gap-1.5">
          <ArrowLeftRightIcon size={14} />
          Switch
        </Button>
      ) : null}
      <RowMenu entry={entry} pending={pending} onView={onView} onEdit={onEdit} onRemove={onRemove} />
    </div>
  );
}

export default function ModelsRegistryTable({ workspaceId, initial, initialSecrets }: Props) {
  const [pending, startTransition] = useTransition();
  const [entries, setEntries] = useState<TenantModelEntry[]>(initial.models);
  const [secrets, setSecrets] = useState<Secret[]>(initialSecrets);
  const [platformDefaultAvailable, setPlatformDefaultAvailable] = useState(initial.platform_default_available);
  const [sort, setSort] = useState<SortState>(null);
  const [error, setError] = useState<string | null>(null);
  const [detailsTarget, setDetailsTarget] = useState<TenantModelEntry | null>(null);
  const [editTarget, setEditTarget] = useState<TenantModelEntry | null>(null);
  const [removeTarget, setRemoveTarget] = useState<TenantModelEntry | null>(null);
  const [removeError, setRemoveError] = useState<string | null>(null);

  const isDefaultLive = !entries.some((e) => e.active);

  const sortedEntries = useMemo(() => {
    if (!sort) return entries;
    const dir = sort.dir === "ascending" ? 1 : -1;
    return [...entries].sort((a, b) => sortValueFor(a, sort.key).localeCompare(sortValueFor(b, sort.key)) * dir);
  }, [entries, sort]);

  const rows: RegistryRow[] = [{ kind: "default" }, ...sortedEntries.map((entry) => ({ kind: "entry" as const, entry }))];

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
    { key: "model", header: "Model", sortable: true, cell: (row) => <ModelCell row={row} /> },
    { key: "provider", header: "Provider", sortable: true, cell: (row) => <ProviderCell row={row} /> },
    { key: "context", header: "Context", numeric: true, hideOnMobile: true, cell: (row) => <span className="font-mono text-xs tabular-nums text-muted-foreground">{row.kind === "entry" ? formatContext(row.entry.context_cap_tokens) : "—"}</span> },
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
        <div className="flex flex-wrap items-baseline justify-between gap-md">
          <SectionLabel>Model registry</SectionLabel>
          <AddModelEntryDialog workspaceId={workspaceId} secrets={secrets} onCreated={refresh} onSecretsChanged={refreshSecrets} />
        </div>

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
