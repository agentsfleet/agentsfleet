"use client";

import { TIME_LABEL } from "@/lib/time-labels";
import { type Ref, useDeferredValue, useImperativeHandle, useMemo, useState, useTransition } from "react";
import { Badge, DataTable, type DataTableColumn, EmptyState, IconAction, Time } from "@agentsfleet/design-system";
import { BanIcon, KeyRoundIcon, Trash2Icon } from "lucide-react";
import { type ApiKeyListResponse, type ApiKeyRow } from "@/lib/api/api_keys";
import { DEFAULT_SORT, type ApiKeySort } from "@/lib/api/api-keys-types";
import { presentErrorString } from "@/lib/errors";
import { CollectionToolbar } from "@/components/domain/CollectionToolbar";
import { listApiKeysAction, revokeApiKeyAction, deleteApiKeyAction } from "../actions";
import RevokeConfirm, { type ConfirmTarget, type ConfirmTargetActive } from "./RevokeConfirm";

// Maps a clicked (sortable) column key to the sort value it should apply next:
// toggles direction if it's already the active column, else defaults to the
// column's "natural" first click (ascending by name, descending by recency).
const NEXT_SORT: Record<"name" | "activity", Record<ApiKeySort, ApiKeySort>> = {
  name: { key_name: "-key_name", "-key_name": "key_name", created_at: "key_name", "-created_at": "key_name" },
  activity: { "-created_at": "created_at", created_at: "-created_at", key_name: "-created_at", "-key_name": "-created_at" },
};

export type ApiKeyListHandle = { refresh: () => void };

// The list is always complete (the server action walks the cursor to the end),
// so a header click reorders it here, at once, instead of asking the server to
// fetch every page again before the arrow could flip.
function sortApiKeys(items: readonly ApiKeyRow[], sort: ApiKeySort): ApiKeyRow[] {
  const direction = sort.startsWith("-") ? -1 : 1;
  const byName = sort === "key_name" || sort === "-key_name";
  // Ties break on the id, so two keys created in the same millisecond keep
  // one order however often the header is clicked.
  return [...items].sort(
    (a, b) =>
      direction * (byName ? a.key_name.localeCompare(b.key_name) : a.created_at - b.created_at) ||
      a.id.localeCompare(b.id),
  );
}

export default function ApiKeyList({
  initial,
  ref,
}: {
  initial: ApiKeyListResponse;
  ref?: Ref<ApiKeyListHandle>;
}) {
  const [pending, startTransition] = useTransition();
  const [items, setItems] = useState<ApiKeyRow[]>(initial.items);
  const [sort, setSort] = useState<ApiKeySort | null>(null);
  const [target, setTarget] = useState<ConfirmTarget>(null);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const query = useDeferredValue(search.trim().toLowerCase());

  // A re-fetch whose failure the user is told about.
  function reload() {
    startTransition(async () => {
      const r = await listApiKeysAction(DEFAULT_SORT);
      if (!r.ok) {
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "load API keys" }));
        return;
      }
      setError(null);
      setItems(r.data.items);
      setSort(null);
    });
  }

  // The header "New API key" dialog (rendered by the parent view) calls this via
  // ref on create — a targeted re-fetch, not a full-route refresh. It returns
  // to the default order so the new key shows first.
  useImperativeHandle(ref, () => ({
    refresh: () => reload(),
  }));

  // Post-mutation re-fetch (Invariant 4): mirror backend reality without
  // clobbering a mutation error the user still needs to read.
  function refresh() {
    startTransition(async () => {
      const r = await listApiKeysAction(DEFAULT_SORT);
      if (r.ok) setItems(r.data.items);
    });
  }

  // `target` is supplied by RevokeConfirm's onConfirm closure (bound only while
  // the target is non-null), so no in-function null check is needed.
  function onConfirm(target: ConfirmTargetActive) {
    const { id, mode } = target;
    setError(null);
    startTransition(async () => {
      const r = mode === "revoke" ? await revokeApiKeyAction(id) : await deleteApiKeyAction(id);
      if (!r.ok) {
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: `${mode} the API key` }));
        refresh();
        return;
      }
      setTarget(null);
      refresh();
    });
  }

  const rows = useMemo(() => sortApiKeys(items.filter((item) => item.key_name.toLowerCase().includes(query)), sort ?? DEFAULT_SORT), [items, sort, query]);
  const sortKey = sort === null ? undefined : sort === "-created_at" || sort === "created_at" ? "activity" : "name";
  const sortDirection = sort === null ? undefined : sort === "key_name" || sort === "created_at" ? "ascending" : "descending";

  return (
    <div className="space-y-4">
      {items.length > 0 ? <CollectionToolbar label="Search API keys" value={search} onChange={setSearch}
        summary={`${rows.length} of ${items.length} keys`} /> : null}
      <DataTable
        isLoading={pending}
        columns={buildColumns({
          pending,
          onRevoke: (k) => setTarget({ ...k, mode: "revoke" }),
          onDelete: (k) => setTarget({ ...k, mode: "delete" }),
        })}
        rows={rows}
        rowKey={(k) => k.id}
        caption="API keys"
        sortKey={sortKey}
        sortDirection={sortDirection}
        onSortChange={(key) => setSort(NEXT_SORT[key as "name" | "activity"][sort ?? DEFAULT_SORT])}
        empty={
          <EmptyState
            icon={<KeyRoundIcon size={28} />}
            title={query ? "No matching keys" : "Connect your first tool"}
            description={query ? "Try another name or clear your search." : "Create an API key for a script, automation, or service. You’ll copy its value once after creating it."}
          />
        }
      />

      {error && target === null ? <p className="text-sm text-destructive">{error}</p> : null}

      {/* Open is controlled by `target`; ConfirmDialog only signals dismissal, so clear unconditionally. */}
      <RevokeConfirm target={target} error={error} onOpenChange={() => { setTarget(null); setError(null); }} onConfirm={onConfirm} />
    </div>
  );
}

function KeyNameCell({ k }: { k: ApiKeyRow }) {
  return (
    <div className="min-w-0 max-w-24 sm:max-w-none">
      <span className="block truncate text-body-sm font-medium">{k.key_name}</span>
      <div className="sm:hidden"><KeyActivityCell k={k} /></div>
    </div>
  );
}

function KeyActivityCell({ k }: { k: ApiKeyRow }) {
  return (
    <div className="flex flex-col items-start gap-xs text-label leading-label text-muted-foreground">
      <span><span className="sm:hidden">Created </span><Time value={new Date(k.created_at)} format="relative" className="tabular-nums" /></span>
      {k.revoked_at ? <span>revoked <Time value={new Date(k.revoked_at)} format="relative" className="tabular-nums" /></span> : null}
    </div>
  );
}

function KeyActionsCell({
  k,
  pending,
  onRevoke,
  onDelete,
}: {
  k: ApiKeyRow;
  pending: boolean;
  onRevoke: () => void;
  onDelete: () => void;
}) {
  // Icon actions matching the catalogue/registry rows — the aria-label
  // carries the verb + key name, the glyph carries the affordance.
  return k.active ? (
    <IconAction type="button" variant="destructive" disabled={pending} onClick={onRevoke} label={`Revoke API key ${k.key_name}`}>
      <BanIcon size={14} />
    </IconAction>
  ) : (
    <IconAction type="button" variant="destructive" disabled={pending} onClick={onDelete} label={`Delete API key ${k.key_name}`}>
      <Trash2Icon size={14} />
    </IconAction>
  );
}

function buildColumns({
  pending,
  onRevoke,
  onDelete,
}: {
  pending: boolean;
  onRevoke: (k: ApiKeyRow) => void;
  onDelete: (k: ApiKeyRow) => void;
}): DataTableColumn<ApiKeyRow>[] {
  return [
    {
      key: "name", primary: true,
      header: "Name",
      cell: (k) => <KeyNameCell k={k} />,
      sortable: true,
    },
    {
      key: "activity",
      header: TIME_LABEL.created,
      hideOnMobile: true,
      cell: (k) => <KeyActivityCell k={k} />,
      sortable: true,
    },
    {
      key: "status",
      header: "Status",
      cell: (k) => <Badge variant={k.active ? "green" : "default"}>{k.active ? "Active" : "Revoked"}</Badge>,
    },
    {
      key: "actions",
      header: "Actions",
      numeric: true,
      cell: (k) => (
        <KeyActionsCell k={k} pending={pending} onRevoke={() => onRevoke(k)} onDelete={() => onDelete(k)} />
      ),
    },
  ];
}
