"use client";

import { useState, useTransition } from "react";
import {
  Badge,
  Button,
  ConfirmDialog,
  DataTable,
  type DataTableColumn,
  EmptyState,
} from "@agentsfleet/design-system";
import { CoinsIcon } from "lucide-react";
import { type AdminModel, nanosToUsdPerMtok } from "@/lib/api/admin_models";
import { presentErrorString } from "@/lib/errors";
import { deleteAdminModelAction } from "../actions";

// $/1M tokens, two decimals — the catalogue is priced per million tokens (matches
// how every provider quotes), so the rates paste straight from a pricing page.
function usd(nanos: number): string {
  return nanosToUsdPerMtok(nanos).toFixed(2);
}

function buildColumns({
  pending,
  busyUid,
  onDelete,
}: {
  pending: boolean;
  busyUid: string | null;
  onDelete: (m: AdminModel) => void;
}): DataTableColumn<AdminModel>[] {
  return [
    { key: "provider", header: "Provider", cell: (m) => <Badge variant="cyan">{m.provider}</Badge> },
    { key: "model", header: "Model", cell: (m) => <span className="font-mono text-sm">{m.model_id}</span> },
    {
      key: "context",
      header: "Context",
      hideOnMobile: true,
      numeric: true,
      cell: (m) => (
        <span className="font-mono text-xs tabular-nums text-muted-foreground">
          {m.context_cap_tokens.toLocaleString()}
        </span>
      ),
    },
    {
      key: "rates",
      header: "Rates ($ / 1M · in / cached / out)",
      numeric: true,
      cell: (m) => (
        <span className="font-mono text-xs tabular-nums text-muted-foreground">
          {usd(m.input_nanos_per_mtok)} / {usd(m.cached_input_nanos_per_mtok)} / {usd(m.output_nanos_per_mtok)}
        </span>
      ),
    },
    {
      key: "actions",
      header: "Actions",
      numeric: true,
      cell: (m) => (
        <Button
          type="button"
          variant="destructive"
          size="sm"
          disabled={pending && busyUid === m.uid}
          onClick={() => onDelete(m)}
        >
          Delete
        </Button>
      ),
    },
  ];
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
  const [target, setTarget] = useState<AdminModel | null>(null);

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
      setTarget(null);
      onDeleted(m.uid);
    });
  }

  const columns = buildColumns({ pending, busyUid, onDelete: setTarget });

  return (
    <div className="space-y-4">
      <DataTable
        columns={columns}
        rows={models}
        rowKey={(m) => m.uid}
        caption="Model rates"
        empty={
          <EmptyState
            icon={<CoinsIcon size={28} />}
            title="No model rates yet"
            description="Add a model rate to price it and make it selectable as the platform default."
          />
        }
      />

      <ConfirmDialog
        open={target !== null}
        onOpenChange={() => {
          setTarget(null);
          setError(null);
        }}
        title={`Delete "${target?.model_id ?? ""}" from the catalogue?`}
        description="Removes this model from the platform catalogue. Tenants can no longer select it as the platform default. This cannot be undone."
        confirmLabel="Delete"
        intent="destructive"
        errorMessage={error}
        onConfirm={target ? () => remove(target) : undefined}
      />
    </div>
  );
}
