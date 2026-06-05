"use client";

import { useState, useTransition } from "react";
import {
  Badge,
  Button,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@usezombie/design-system";
import {
  API_KEY_SORTS,
  DEFAULT_PAGE_SIZE,
  DEFAULT_SORT,
  type ApiKeyListResponse,
  type ApiKeyRow,
  type ApiKeySort,
} from "@/lib/api/api_keys";
import { presentErrorString } from "@/lib/errors";
import { listApiKeysAction, revokeApiKeyAction, deleteApiKeyAction } from "../actions";
import CreateApiKeyDialog from "./CreateApiKeyDialog";
import RevokeConfirm, { type ConfirmTarget } from "./RevokeConfirm";

const SORT_LABELS: Record<ApiKeySort, string> = {
  "-created_at": "Newest first",
  created_at: "Oldest first",
  key_name: "Name A–Z",
  "-key_name": "Name Z–A",
};

// Callers always pass a present epoch (created_at) or pre-guard the nullable
// fields (last_used_at / revoked_at), so `ms` is never null here.
function fmt(ms: number): string {
  return new Date(ms).toLocaleString();
}

export default function ApiKeyList({ initial }: { initial: ApiKeyListResponse }) {
  const [pending, startTransition] = useTransition();
  const [items, setItems] = useState<ApiKeyRow[]>(initial.items);
  const [total, setTotal] = useState(initial.total);
  const [page, setPage] = useState(initial.page);
  const [sort, setSort] = useState<ApiKeySort>(DEFAULT_SORT);
  const [target, setTarget] = useState<ConfirmTarget>(null);
  const [error, setError] = useState<string | null>(null);

  const lastPage = Math.max(1, Math.ceil(total / DEFAULT_PAGE_SIZE));

  function apply(data: ApiKeyListResponse, nextSort: ApiKeySort) {
    setItems(data.items);
    setTotal(data.total);
    setPage(data.page);
    setSort(nextSort);
  }

  // User-initiated sort/page navigation. Clears the error on a clean load; an
  // invalid sort/page (UZ-REQ-001) resets to the defaults rather than blanking.
  // `retried` guards the reset: if the backend rejects even the defaults
  // (contract drift), self-calling again would loop forever — reset at most once.
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

  function onConfirm() {
    if (!target) return;
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

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Select value={sort} onValueChange={(v) => loadPage({ sort: v as ApiKeySort, page: 1 })}>
          <SelectTrigger className="w-44" aria-label="Sort API keys">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {API_KEY_SORTS.map((s) => (
              <SelectItem key={s} value={s}>
                {SORT_LABELS[s]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <CreateApiKeyDialog onCreated={() => loadPage({ page: 1, sort: DEFAULT_SORT })} />
      </div>

      {items.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No API keys yet. Create one to authenticate a service-to-service caller.
        </p>
      ) : (
        <div className="divide-y rounded-md border">
          {items.map((k) => (
            <Row key={k.id} k={k} pending={pending} onRevoke={() => setTarget({ ...k, mode: "revoke" })} onDelete={() => setTarget({ ...k, mode: "delete" })} />
          ))}
        </div>
      )}

      {error && target === null ? <p className="text-sm text-destructive">{error}</p> : null}

      {lastPage > 1 ? (
        <div className="flex items-center justify-between text-sm text-muted-foreground">
          <span>
            {/* Always plural: this block only renders when lastPage > 1, i.e. total > one page. */}
            Page {page} of {lastPage} · {total} keys
          </span>
          <div className="flex gap-2">
            <Button type="button" variant="ghost" size="sm" disabled={pending || page <= 1} onClick={() => loadPage({ page: page - 1 })}>
              Previous
            </Button>
            <Button type="button" variant="ghost" size="sm" disabled={pending || page >= lastPage} onClick={() => loadPage({ page: page + 1 })}>
              Next
            </Button>
          </div>
        </div>
      ) : null}

      {/* Open is controlled by `target`; ConfirmDialog only signals dismissal, so clear unconditionally. */}
      <RevokeConfirm target={target} error={error} onOpenChange={() => { setTarget(null); setError(null); }} onConfirm={onConfirm} />
    </div>
  );
}

function Row({ k, pending, onRevoke, onDelete }: { k: ApiKeyRow; pending: boolean; onRevoke: () => void; onDelete: () => void }) {
  return (
    <div className="flex items-center justify-between gap-3 p-3">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className="truncate font-mono text-sm">{k.key_name}</span>
          <Badge variant={k.active ? "green" : "amber"}>{k.active ? "active" : "revoked"}</Badge>
        </div>
        <div className="font-mono text-xs tabular-nums text-muted-foreground">
          created {fmt(k.created_at)} · {k.last_used_at ? `last used ${fmt(k.last_used_at)}` : "never used"}
          {k.revoked_at ? ` · revoked ${fmt(k.revoked_at)}` : ""}
        </div>
      </div>
      {k.active ? (
        <Button type="button" variant="ghost" size="sm" disabled={pending} onClick={onRevoke} aria-label={`Revoke API key ${k.key_name}`}>
          Revoke
        </Button>
      ) : (
        <Button type="button" variant="ghost" size="sm" disabled={pending} onClick={onDelete} aria-label={`Delete API key ${k.key_name}`}>
          Delete
        </Button>
      )}
    </div>
  );
}
