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
import { type AdminModel, type PlatformKey } from "@/lib/api/admin_model_library";
import {
  CONTEXT_HEADER,
  RATES_HEADER,
  compareModels,
  compareNanos,
  currentBasePrice,
  formatContextTokens,
  formatRatesPerMtok,
} from "@/lib/models/display";
import { CatalogueEvidence, ModelPriceSchedule } from "@/components/domain/ModelPriceSchedule";
import { ModelName, ProviderBadge } from "@/components/domain/ModelCells";
import { providerLabel } from "@/lib/api/model-library-types";
import { presentErrorString } from "@/lib/errors";
import {
  default as EditModelDialogDynamic,
  preloadEditModelDialog,
} from "@/components/domain/island-dynamic/EditModelDialogDynamic";
import { maySpeculateOnHover } from "@/components/domain/island-dynamic/intent-module-loader";
import { deleteAdminModelAction } from "../actions";
import MakeDefaultDialog from "./MakeDefaultDialog";

// The active default resolves to exactly one catalogue row — the one whose
// (provider, model_id) equals the active row's (provider, model).
function isDefault(m: AdminModel, active: PlatformKey | null): boolean {
  return active !== null && active.provider === m.provider && active.model === m.model_id;
}

function ModelCell({ model, active }: { model: AdminModel; active: PlatformKey | null }) {
  return (
    <ModelName
      trimmed
      modelId={model.model_id}
      displayName={model.metadata.display_name}
      trailing={isDefault(model, active) ? <Badge variant="cyan">Default</Badge> : null}
    />
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
        onFocus={preloadEditModelDialog}
        onPointerEnter={() => {
          if (maySpeculateOnHover()) preloadEditModelDialog();
        }}
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
  busyId,
  onEdit,
  onMakeDefault,
  onDelete,
}: {
  active: PlatformKey | null;
  pending: boolean;
  busyId: string | null;
  onEdit: (m: AdminModel) => void;
  onMakeDefault: (m: AdminModel) => void;
  onDelete: (m: AdminModel) => void;
}): DataTableColumn<AdminModel>[] {
  return [
    {
      key: "provider",
      header: "Provider",
      // By the name the badge shows, so the column reads in order.
      sortValue: (m) => providerLabel(m.provider),
      cell: (m) => <ProviderBadge provider={m.provider} />,
    },
    {
      key: "model", primary: true,
      header: "Model",
      sortValue: (m) => m.model_id,
      compare: (a, b) => compareModels({ model_id: a.model_id, display_name: a.metadata.display_name },
        { model_id: b.model_id, display_name: b.metadata.display_name }),
      cell: (m) => <ModelCell model={m} active={active} />,
    },
    {
      key: "context",
      header: CONTEXT_HEADER,
      hideOnMobile: true,
      numeric: true,
      sortValue: (m) => m.metadata.context_cap_tokens,
      cell: (m) => (
        <div className="font-mono text-mono leading-mono tabular-nums text-muted-foreground">
          {formatContextTokens(m.metadata.context_cap_tokens)}
          <CatalogueEvidence source={m.metadata.context_source_url} verifiedAt={m.metadata.context_verified_at} />
        </div>
      ),
    },
    {
      key: "rates",
      header: RATES_HEADER,
      numeric: true,
      sortValue: (m) => currentBasePrice(m.prices)?.input_nanos_per_mtok ?? "",
      compare: (a, b) => compareNanos(currentBasePrice(a.prices)?.input_nanos_per_mtok ?? null, currentBasePrice(b.prices)?.input_nanos_per_mtok ?? null),
      cell: (m) => (
        <div className="space-y-sm">
          <span className="font-mono text-mono leading-mono tabular-nums text-muted-foreground">{formatRatesPerMtok(currentBasePrice(m.prices))}</span>
          <ModelPriceSchedule prices={m.prices} />
        </div>
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
          busy={pending && busyId === m.id}
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
  onDeleted: (id: string) => void;
  onUpdated: (m: AdminModel) => void;
}) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [target, setTarget] = useState<AdminModel | null>(null);
  const [editTarget, setEditTarget] = useState<AdminModel | null>(null);
  const [defaultTarget, setDefaultTarget] = useState<AdminModel | null>(null);

  function remove(m: AdminModel) {
    setError(null);
    setBusyId(m.id);
    startTransition(async () => {
      const r = await deleteAdminModelAction(m.id);
      setBusyId(null);
      if (!r.ok) {
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "delete this model" }));
        return;
      }
      setTarget(null);
      onDeleted(m.id);
    });
  }

  const columns = buildColumns({
    active: activeDefault,
    pending,
    busyId,
    onEdit: setEditTarget,
    onMakeDefault: setDefaultTarget,
    onDelete: setTarget,
  });

  return (
    // Ends at its rows, as the Fleet library tables do: nothing here grows the
    // table, and `min-h-0` lets a library longer than the screen shrink so the
    // DataTable's own viewport scrolls its rows with the pagination bar in view.
    <div className="flex min-h-0 flex-col">
      <DataTable
        columns={columns}
        rows={models}
        rowKey={(m) => m.id}
        caption="Model library"
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
        <EditModelDialogDynamic
          key={editTarget.id}
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
          key={defaultTarget.id}
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
