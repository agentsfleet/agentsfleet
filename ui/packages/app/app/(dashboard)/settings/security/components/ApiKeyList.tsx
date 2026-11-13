"use client";

import { type Ref, useImperativeHandle, useState, useTransition } from "react";
import { Button, DashboardPanel, EmptyState, Time } from "@agentsfleet/design-system";
import { KeyIcon } from "@phosphor-icons/react/ssr";
import { type ApiKeyListResponse, type ApiKeyRow } from "@/lib/api/api_keys";
import { DEFAULT_SORT } from "@/lib/api/api-keys-types";
import { presentErrorString } from "@/lib/errors";
import { listApiKeysAction, revokeApiKeyAction, deleteApiKeyAction } from "../actions";
import RevokeConfirm, { type ConfirmTarget, type ConfirmTargetActive } from "./RevokeConfirm";

export type ApiKeyListHandle = { refresh: () => void };

export default function ApiKeyList({
  initial,
  ref,
}: {
  initial: ApiKeyListResponse;
  ref?: Ref<ApiKeyListHandle>;
}) {
  const [pending, startTransition] = useTransition();
  const [mutating, setMutating] = useState(false);
  const [items, setItems] = useState<ApiKeyRow[]>(initial.items);
  const [target, setTarget] = useState<ConfirmTarget>(null);
  const [error, setError] = useState<string | null>(null);

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
  async function onConfirm(target: ConfirmTargetActive) {
    const { id, mode } = target;
    setMutating(true);
    setError(null);
    try {
      const r = mode === "revoke" ? await revokeApiKeyAction(id) : await deleteApiKeyAction(id);
      if (!r.ok) {
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: `${mode} the API key` }));
        refresh();
        return;
      }
      setTarget(null);
      refresh();
    } catch {
      setError("The key change could not be confirmed. Refresh before trying again.");
      refresh();
    } finally { setMutating(false); }
  }

  return (
    <div className="space-y-xl" aria-busy={pending || mutating}>
      {items.length ? <DashboardPanel padding="none"><ul className="divide-y divide-border">
        {items.map(key => <li key={key.id} className="flex min-w-0 flex-wrap items-center gap-xl p-xl sm:p-5">
          <span aria-hidden="true" className="hidden size-10 items-center sm:flex justify-center rounded-control border border-border"><KeyIcon size={16} /></span>
          <div className="min-w-0 basis-full sm:flex-1 sm:basis-auto"><p className="break-words text-label font-medium">{key.key_name}</p>
            <p className="mt-sm text-label text-muted-foreground">Created <Time value={key.created_at} format="date" tooltip />{key.active ? null : <> · Revoked{key.revoked_at ? <> <Time value={key.revoked_at} format="date" tooltip /></> : null}</>}</p></div>
          <Button variant="outline" disabled={pending || mutating} onClick={() => setTarget({...key, mode: key.active ? "revoke" : "delete"})}
            aria-label={`${key.active ? "Revoke" : "Delete"} API key ${key.key_name}`}>{key.active ? "Revoke" : "Delete"}</Button>
        </li>)}
      </ul></DashboardPanel> : <EmptyState title="No API keys" description="Create one when you need to connect a tool." />}

      {error && target === null ? <p className="text-sm text-destructive">{error}</p> : null}

      {/* Open is controlled by `target`; ConfirmDialog only signals dismissal, so clear unconditionally. */}
      <RevokeConfirm target={target} error={error} onOpenChange={() => { setTarget(null); setError(null); }} onConfirm={onConfirm} />
    </div>
  );
}

