"use client";

import { useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import {
  Badge,
  ConfirmDialog,
  DataTable,
  type DataTableColumn,
  EmptyState,
  IconAction,
  Spinner,
} from "@agentsfleet/design-system";
import { CoinsIcon, PencilIcon, StarIcon, Trash2Icon } from "lucide-react";
import { type AdminModel, type PlatformKey, nanosToUsdPerMtok } from "@/lib/api/admin_model_library";
import { presentErrorString } from "@/lib/errors";
import { deleteAdminModelAction } from "../actions";
import EditModelDialog from "./EditModelDialog";
import MakeDefaultDialog from "./MakeDefaultDialog";

// $/1M tokens, two decimals — the catalogue is priced per million tokens (matches
// how every provider quotes), so the rates paste straight from a pricing page.
function usd(nanos: number): string {
  return nanosToUsdPerMtok(nanos).toFixed(2);
}

// The active default resolves to exactly one catalogue row — the one whose
// (provider, model_id) equals the active row's (provider, model).
function isDefault(m: AdminModel, active: PlatformKey | null): boolean {
  return active !== null && active.provider === m.provider && active.model === m.model_id;
}

function ModelCell({ model, active }: { model: AdminModel; active: PlatformKey | null }) {
  return (
    <span className="flex items-center gap-2">
      <span className="font-mono text-sm">{model.model_id}</span>
      {isDefault(model, active) ? <Badge variant="cyan">Default</Badge> : null}
    </span>
  );
}

function RowActions({
  model,
  active,
  busy,
  onEdit,
  onMakeDefault,
  onDelete,
}: {
  model: AdminModel;
  active: PlatformKey | null;
  // True only while THIS row's delete is in flight — disables the row's actions
  // and swaps the trash icon for a spinner.
  busy: boolean;
  onEdit: (m: AdminModel) => void;
  onMakeDefault: (m: AdminModel) => void;
  onDelete: (m: AdminModel) => void;
}) {
  return (
    <div className="flex justify-end gap-1">
      <IconAction
        type="button"
        variant="ghost"
        onClick={() => onEdit(model)}
        disabled={busy}
        label={`Edit ${model.model_id}`}
      >
        <PencilIcon size={14} />
      </IconAction>
      {isDefault(model, active) ? null : (
        <IconAction
          type="button"
          variant="ghost"
          onClick={() => onMakeDefault(model)}
          disabled={busy}
          label={`Make ${model.model_id} the platform default`}
        >
          <StarIcon size={14} />
        </IconAction>
      )}
      <IconAction
        type="button"
        variant="destructive"
        disabled={busy}
        onClick={() => onDelete(model)}
        label={`Delete ${model.model_id}`}
      >
        {busy ? <Spinner size="sm" srLabel="Deleting" /> : <Trash2Icon size={14} />}
      </IconAction>
    </div>
  );
}

function buildColumns({
  active,
  pending,
  busyUid,
  onEdit,
  onMakeDefault,
  onDelete,
}: {
  active: PlatformKey | null;
  pending: boolean;
  busyUid: string | null;
  onEdit: (m: AdminModel) => void;
  onMakeDefault: (m: AdminModel) => void;
  onDelete: (m: AdminModel) => void;
}): DataTableColumn<AdminModel>[] {
  return [
    { key: "provider", header: "Provider", sortValue: (m) => m.provider, cell: (m) => <Badge variant="cyan">{m.provider}</Badge> },
    { key: "model", header: "Model", sortValue: (m) => m.model_id, cell: (m) => <ModelCell model={m} active={active} /> },
    {
      key: "context",
      header: "Context",
      hideOnMobile: true,
      numeric: true,
      sortValue: (m) => m.context_cap_tokens,
      cell: (m) => (
        <span className="font-mono text-xs tabular-nums text-muted-foreground">
          {/* Pin the locale — a bare toLocaleString() groups digits per the
              viewer's locale (en-IN "1,28,000" vs en-US "128,000"), so SSR and
              client disagree and React throws a hydration mismatch. */}
          {m.context_cap_tokens.toLocaleString("en-US")}
        </span>
      ),
    },
    {
      key: "rates",
      header: "Rates ($ / 1M · in / cached / out)",
      numeric: true,
      sortValue: (m) => m.input_nanos_per_mtok,
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
        <RowActions
          model={m}
          active={active}
          busy={pending && busyUid === m.uid}
          onEdit={onEdit}
          onMakeDefault={onMakeDefault}
          onDelete={onDelete}
        />
      ),
    },
  ];
}

export default function CatalogueList({
  models,
  activeDefault,
  onDeleted,
  onUpdated,
}: {
  models: AdminModel[];
  activeDefault: PlatformKey | null;
  onDeleted: (uid: string) => void;
  onUpdated: (m: AdminModel) => void;
}) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  const [error, setError] = useState<string | null>(null);
  const [busyUid, setBusyUid] = useState<string | null>(null);
  const [target, setTarget] = useState<AdminModel | null>(null);
  const [editTarget, setEditTarget] = useState<AdminModel | null>(null);
  const [defaultTarget, setDefaultTarget] = useState<AdminModel | null>(null);

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

  const columns = buildColumns({
    active: activeDefault,
    pending,
    busyUid,
    onEdit: setEditTarget,
    onMakeDefault: setDefaultTarget,
    onDelete: setTarget,
  });

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <DataTable
        className="flex min-h-0 flex-1 flex-col"
        columns={columns}
        rows={models}
        rowKey={(m) => m.uid}
        caption="Model library"
        viewportClassName="min-h-0 flex-1 max-h-none"
        empty={
          <EmptyState
            icon={<CoinsIcon size={28} />}
            title="No models yet"
            description="Add a model to price it and make it the platform default."
          />
        }
      />

      {editTarget ? (
        // Mounted only while editing and controlled always-open, so Radix only
        // ever signals a close (onOpenChange(false)) — unmount on any close signal.
        <EditModelDialog
          key={editTarget.uid}
          model={editTarget}
          onOpenChange={() => setEditTarget(null)}
          onUpdated={(m) => {
            onUpdated(m);
            setEditTarget(null);
          }}
        />
      ) : null}

      {defaultTarget ? (
        <MakeDefaultDialog
          key={defaultTarget.uid}
          model={defaultTarget}
          onOpenChange={() => setDefaultTarget(null)}
          onDone={() => {
            setDefaultTarget(null);
            // Re-read the server so the "Default" badge moves to this row.
            router.refresh();
          }}
        />
      ) : null}

      <ConfirmDialog
        open={target !== null}
        onOpenChange={() => {
          setTarget(null);
          setError(null);
        }}
        title={`Delete "${target?.model_id ?? ""}" from the library?`}
        description="Removes this model from the platform library. Tenants can no longer select it as the platform default. This cannot be undone."
        confirmLabel="Delete"
        intent="destructive"
        errorMessage={error}
        onConfirm={target ? () => remove(target) : undefined}
      />
    </div>
  );
}
