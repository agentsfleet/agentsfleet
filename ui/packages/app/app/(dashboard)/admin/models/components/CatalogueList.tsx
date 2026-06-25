"use client";

import { useState, useTransition } from "react";
import { Badge, Button, EmptyState } from "@agentsfleet/design-system";
import { LayersIcon } from "lucide-react";
import { type AdminModel, nanosToUsdPerMtok } from "@/lib/api/admin_models";
import { presentErrorString } from "@/lib/errors";
import { deleteAdminModelAction } from "../actions";

// $/1M tokens, two decimals — the catalogue is priced per million tokens (matches
// how every provider quotes), so the rates paste straight from a pricing page.
function usd(nanos: number): string {
  return nanosToUsdPerMtok(nanos).toFixed(2);
}

export default function CatalogueList({
  models,
  onDeleted,
}: {
  models: AdminModel[];
  onDeleted: (uid: string) => void;
}) {
  const [pending, startTransition] = useTransition();
  const [error, setError] = useState<string | null>(null);
  const [busyUid, setBusyUid] = useState<string | null>(null);

  function remove(m: AdminModel) {
    setError(null);
    setBusyUid(m.uid);
    startTransition(async () => {
      const r = await deleteAdminModelAction(m.uid);
      setBusyUid(null);
      if (!r.ok) {
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "delete this model" }));
        return;
      }
      onDeleted(m.uid);
    });
  }

  return (
    <div className="space-y-4">
      <p className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
        Model catalogue · {models.length} {models.length === 1 ? "model" : "models"}
      </p>

      {models.length === 0 ? (
        <EmptyState
          icon={<LayersIcon size={28} />}
          title="No models yet"
          description="Add a model to price it and make it selectable as the platform default."
        />
      ) : (
        <div className="divide-y rounded-md border">
          <div className="hidden grid-cols-[1fr_1.6fr_0.8fr_1.6fr_auto] gap-3 px-3 py-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground sm:grid">
            <span>Provider</span>
            <span>Model</span>
            <span>Context</span>
            <span>Rates ($ / 1M · in / cached / out)</span>
            <span className="sr-only">Actions</span>
          </div>
          {models.map((m) => (
            <div
              key={m.uid}
              className="grid grid-cols-1 gap-2 p-3 sm:grid-cols-[1fr_1.6fr_0.8fr_1.6fr_auto] sm:items-center sm:gap-3"
              aria-label={`${m.provider} ${m.model_id} catalogue row`}
            >
              <span><Badge variant="cyan">{m.provider}</Badge></span>
              <span className="truncate font-mono text-sm">{m.model_id}</span>
              <span className="font-mono text-xs tabular-nums text-muted-foreground">
                {m.context_cap_tokens.toLocaleString()}
              </span>
              <span className="font-mono text-xs tabular-nums text-muted-foreground">
                {usd(m.input_nanos_per_mtok)} / {usd(m.cached_input_nanos_per_mtok)} / {usd(m.output_nanos_per_mtok)}
              </span>
              <span className="sm:justify-self-end">
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  disabled={pending && busyUid === m.uid}
                  onClick={() => remove(m)}
                >
                  Delete
                </Button>
              </span>
            </div>
          ))}
        </div>
      )}

      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      <p className="text-xs text-muted-foreground">
        Adding or removing a model updates the live rate cache immediately. Deleting the model the
        platform default points at is blocked — repoint the default first.
      </p>
    </div>
  );
}
