"use client";

import { type Ref, useImperativeHandle, useState, useTransition } from "react";
import { Badge, Button, DataTable, type DataTableColumn, EmptyState, PAGINATION_KIND, Time } from "@agentsfleet/design-system";
import { BanIcon, KeyRoundIcon, Trash2Icon } from "lucide-react";
import {
  DEFAULT_PAGE_SIZE,
  DEFAULT_SORT,
  type ApiKeyListResponse,
  type ApiKeyRow,
  type ApiKeySort,
} from "@/lib/api/api_keys";
import { presentErrorString } from "@/lib/errors";
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

export default function ApiKeyList({
  initial,
  ref,
}: {
  initial: ApiKeyListResponse;
  ref?: Ref<ApiKeyListHandle>;
}) {
  const [pending, startTransition] = useTransition();
  const [items, setItems] = useState<ApiKeyRow[]>(initial.items);
  const [total, setTotal] = useState(initial.total);
  const [page, setPage] = useState(initial.page);
  const [sort, setSort] = useState<ApiKeySort>(DEFAULT_SORT);
  const [target, setTarget] = useState<ConfirmTarget>(null);
  const [error, setError] = useState<string | null>(null);

  // The header "New API key" dialog (rendered by the parent view) calls this via
  // ref on create — a targeted re-fetch of page 1, not a full-route refresh.
  useImperativeHandle(ref, () => ({
    refresh: () => loadPage({ page: 1, sort: DEFAULT_SORT }),
  }));

  function apply(data: ApiKeyListResponse, nextSort: ApiKeySort) {
    setItems(data.items);
    setTotal(data.total);
    setPage(data.page);
    setSort(nextSort);
  }

  // User-initiated sort/page navigation. Clears the error on a clean load; an
  // invalid sort/page (UZ-REQ-001) resets to the defaults rather than blanking.
  // `retried` guards the reset: if the backend rejects even the defaults
  // (response drift), self-calling again would loop forever — reset at most once.
  function loadPage(next: { page: number; sort?: ApiKeySort }, retried = false) {
    const nextPage = next.page;
    const nextSort = next.sort ?? sort;
    startTransition(async () => {
      const r = await listApiKeysAction({ page: nextPage, page_size: DEFAULT_PAGE_SIZE, sort: nextSort });
      if (!r.ok) {
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "load API keys" }));
        if (r.errorCode === "UZ-REQ-001" && !retried) loadPage({ page: 1, sort: DEFAULT_SORT }, true);
        return;
      }
      setError(null);
      apply(r.data, nextSort);
    });
  }

  // Post-mutation re-fetch (Invariant 4): mirror backend reality without
  // clobbering a mutation error the user still needs to read.
  function refresh() {
    startTransition(async () => {
      const r = await listApiKeysAction({ page, page_size: DEFAULT_PAGE_SIZE, sort });
      if (r.ok) apply(r.data, sort);
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

  const sortKey = sort === "-created_at" || sort === "created_at" ? "activity" : "name";
  const sortDirection = sort === "key_name" || sort === "created_at" ? "ascending" : "descending";

  return (
    <div className="space-y-4">
      <DataTable
        columns={buildColumns({
          pending,
          onRevoke: (k) => setTarget({ ...k, mode: "revoke" }),
          onDelete: (k) => setTarget({ ...k, mode: "delete" }),
        })}
        rows={items}
        rowKey={(k) => k.id}
        caption="API keys"
        sortKey={sortKey}
        sortDirection={sortDirection}
        onSortChange={(key) => loadPage({ sort: NEXT_SORT[key as "name" | "activity"][sort], page: 1 })}
        pagination={{
          kind: PAGINATION_KIND.page,
          page,
          pageSize: DEFAULT_PAGE_SIZE,
          total,
          onPageChange: (nextPage) => loadPage({ page: nextPage }),
          isLoading: pending,
        }}
        empty={
          <EmptyState
            icon={<KeyRoundIcon size={28} />}
            title="No API keys yet"
            description="Create one for outside tools."
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
    <div className="flex items-center gap-2 min-w-0">
      <span className="truncate font-mono text-sm">{k.key_name}</span>
      <Badge variant={k.active ? "green" : "amber"}>{k.active ? "active" : "revoked"}</Badge>
    </div>
  );
}

// created_at is always a present epoch; last_used_at / revoked_at are pre-guarded
// as nullable, so Time renders only for a real timestamp (never for null).
function KeyActivityCell({ k }: { k: ApiKeyRow }) {
  return (
    <span className="font-mono text-xs tabular-nums text-muted-foreground">
      created <Time value={new Date(k.created_at)} format="relative" /> ·{" "}
      {k.last_used_at ? (
        <>
          last used <Time value={new Date(k.last_used_at)} format="relative" />
        </>
      ) : (
        "never used"
      )}
      {k.revoked_at ? (
        <>
          {" · "}revoked <Time value={new Date(k.revoked_at)} format="relative" />
        </>
      ) : null}
    </span>
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
    <Button type="button" variant="destructive" size="sm" disabled={pending} onClick={onRevoke} aria-label={`Revoke API key ${k.key_name}`}>
      <BanIcon size={14} />
    </Button>
  ) : (
    <Button type="button" variant="destructive" size="sm" disabled={pending} onClick={onDelete} aria-label={`Delete API key ${k.key_name}`}>
      <Trash2Icon size={14} />
    </Button>
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
      key: "name",
      header: "Name",
      cell: (k) => <KeyNameCell k={k} />,
      sortable: true,
    },
    {
      key: "activity",
      header: "Created",
      cell: (k) => <KeyActivityCell k={k} />,
      sortable: true,
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
