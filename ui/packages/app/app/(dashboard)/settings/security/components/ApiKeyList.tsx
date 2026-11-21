"use client";

import { type Ref, useImperativeHandle, useState, useTransition } from "react";
import { Button, DashboardPanel, EmptyState, Time } from "@agentsfleet/design-system";
import { SecurityResourceRow } from "@/components/account/SecurityResourceRow";
import { KeyIcon } from "@phosphor-icons/react/ssr";
import { type ApiKeyListResponse, type ApiKeyRow } from "@/lib/api/api_keys";
import { DEFAULT_SORT, KEY_PREFIX, KEY_SUFFIX_REGEX } from "@/lib/api/api-keys-types";
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
        {items.map(key => (
          <KeyRow key={key.id} row={key} disabled={pending || mutating} onAction={setTarget} />
        ))}
      </ul></DashboardPanel> : <EmptyState title="No API keys" description="Create one when you need to connect a tool." />}

      {error && target === null ? <p className="text-sm text-destructive">{error}</p> : null}

      {/* Open is controlled by `target`; ConfirmDialog only signals dismissal, so clear unconditionally. */}
      <RevokeConfirm target={target} error={error} onOpenChange={() => { setTarget(null); setError(null); }} onConfirm={onConfirm} />
    </div>
  );
}

function KeyRow({row, disabled, onAction}: {row: ApiKeyRow; disabled: boolean; onAction: (target: ConfirmTargetActive) => void}) {
  const mode = row.active ? "revoke" : "delete";
  const action = row.active ? "Revoke" : "Delete";
  const detail = (
    <>
      <KeyIdentifier suffix={row.display_suffix} /> · <span className="inline-block">Created <Time value={row.created_at} format="date" tooltip /></span>
      {!row.active && <> · <span className="inline-block">Revoked{row.revoked_at && <> <Time value={row.revoked_at} format="date" tooltip /></>}</span></>}
    </>
  );
  return (
    <SecurityResourceRow kind="key" title={row.key_name} icon={<KeyIcon size={16} />} detail={detail}
      lastUsed="Last used: unavailable"
      action={<Button variant="outline" disabled={disabled} aria-label={`${action} API key ${row.key_name}`}
        onClick={() => onAction({...row, mode})}>{action}</Button>} />
  );
}

function KeyIdentifier({suffix}: {suffix: string | null}) {
  return typeof suffix === "string" && KEY_SUFFIX_REGEX.test(suffix)
    ? <code className="font-mono text-mono">{KEY_PREFIX}••••••••{suffix}</code>
    : <span>Identifier unavailable</span>;
}

