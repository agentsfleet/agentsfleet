"use client";

import {
  Badge,
  Button,
  CopyButton,
  IconAction,
} from "@agentsfleet/design-system";
import { ArrowLeftRightIcon, EyeIcon, PencilIcon, Trash2Icon } from "lucide-react";
import { OLLAMA_PROVIDER, type LibraryModel } from "@/lib/api/model-library-types";
import {
  EMPTY_VALUE,
  PRICE_UNKNOWN,
  formatContextTokens,
  formatRatesPerMtok,
  type RateFields,
} from "@/lib/models/display";
import { ModelName, PlatformManagedMark, ProviderBadge } from "@/components/domain/ModelCells";
import { libraryRateFor, rowRateFor } from "./registry-view";
import type { TenantModelEntry, TenantPlatformDefault } from "@/lib/types";

// Presentational cells + row actions for ModelsRegistryTable, split out so the
// stateful table module stays under the file-length cap. Everything here is
// pure props → markup; state and server actions live in the table.

export type RegistryRow = { kind: "default" } | { kind: "entry"; entry: TenantModelEntry };

const PLATFORM_UNAVAILABLE_NOTE = "No default is configured.";
const RATES_UNAVAILABLE = "Rates unavailable";
const NUMERIC_CELL_CLASS = "font-mono text-mono leading-mono tabular-nums text-muted-foreground";
// Self-managed rows use the tenant's provider account.
const RATES_NOT_APPLICABLE = "Billed by provider";

export function rowKey(row: RegistryRow): string {
  return row.kind === "default" ? "default" : row.entry.id;
}

export function ModelCell({
  row,
  platformDefault,
}: {
  row: RegistryRow;
  platformDefault: TenantPlatformDefault | null;
}) {
  if (row.kind === "entry") return <ModelName modelId={row.entry.model_id} displayName={row.entry.display_name} />;
  // The lock is the whole statement: this row is the platform default and a
  // tenant admin cannot edit it. With no default configured the lock stands
  // alone; the actions column says why.
  if (!platformDefault) return <PlatformManagedMark />;
  return <ModelName modelId={platformDefault.model} displayName={platformDefault.display_name} managedByPlatform />;
}

export function ProviderCell({
  row,
  platformDefault,
}: {
  row: RegistryRow;
  platformDefault: TenantPlatformDefault | null;
}) {
  if (row.kind === "default") {
    return <div className="min-w-0">{platformDefault ? <ProviderBadge provider={platformDefault.provider} /> : null}</div>;
  }
  const { entry } = row;
  return (
    <div className="flex min-w-0 flex-col items-start gap-1">
      <ProviderBadge provider={entry.provider} />
      {entry.base_url ? (
        <div className="flex min-w-0 items-center gap-1">
          <div className="truncate font-mono text-mono leading-mono text-muted-foreground">{entry.base_url}</div>
          <CopyButton value={entry.base_url} label={`Copy base URL: ${entry.base_url}`} />
        </div>
      ) : null}
    </div>
  );
}

/** Context cap over the library's per-token rates — the Default row reads
 * both from the ridden-along platform default identity. */
type ModelIdentity = {
  provider: string | undefined;
  model: string;
  context: number | undefined;
  rate: RateFields | null;
};

/**
 * The row's model, whichever kind of row it is.
 *
 * Both the context and the price cell need exactly this, and resolving it twice
 * is how the two columns would drift apart when the Default row's shape changes.
 */
function identityFor(
  row: RegistryRow,
  platformDefault: TenantPlatformDefault | null,
): ModelIdentity | null {
  if (row.kind === "default") {
    return platformDefault
      ? {
          provider: platformDefault.provider,
          model: platformDefault.model,
          context: platformDefault.context_cap_tokens,
          rate: rowRateFor(platformDefault),
        }
      : null;
  }
  return {
    provider: row.entry.provider,
    model: row.entry.model_id,
    context: row.entry.context_cap_tokens,
    rate: rowRateFor(row.entry),
  };
}

/** The context window in tokens, grouped as the model library prints it. */
export function ContextCell({
  row,
  platformDefault,
}: {
  row: RegistryRow;
  platformDefault: TenantPlatformDefault | null;
}) {
  const identity = identityFor(row, platformDefault);
  return (
    <span className={NUMERIC_CELL_CLASS}>
      {identity ? formatContextTokens(identity.context) : EMPTY_VALUE}
    </span>
  );
}

/** Per-million pricing, split out of the context column it used to share. */
export function RatesCell({
  row,
  platformDefault,
  libraryModels,
}: {
  row: RegistryRow;
  platformDefault: TenantPlatformDefault | null;
  libraryModels: LibraryModel[];
}) {
  const identity = identityFor(row, platformDefault);
  if (!identity) return <span className={NUMERIC_CELL_CLASS}>{EMPTY_VALUE}</span>;
  if (identity.provider === OLLAMA_PROVIDER) return <span className={NUMERIC_CELL_CLASS}>{PRICE_UNKNOWN}</span>;
  const rate = identity.rate ?? libraryRateFor(libraryModels, identity.provider, identity.model);
  // A tenant entry is self-managed by definition — the platform row is the only
  // platform-managed one — so an unpriced entry is "not applicable", not a miss.
  const rateLine = rate
    ? formatRatesPerMtok(rate)
    : row.kind === "entry"
      ? RATES_NOT_APPLICABLE
      : RATES_UNAVAILABLE;
  return <span className={NUMERIC_CELL_CLASS}>{rateLine}</span>;
}

export function StatusCell({ row, isDefaultLive }: { row: RegistryRow; isDefaultLive: boolean }) {
  if (row.kind === "default") return isDefaultLive ? <Badge variant="green">Active</Badge> : null;
  const { entry } = row;
  if (entry.active) return <Badge variant="green">Active</Badge>;
  if (!entry.has_key) return <Badge variant="default">no key · local</Badge>;
  return null;
}

export function ActionsCell({
  row,
  pending,
  isDefaultLive,
  platformDefaultAvailable,
  onSwitchDefault,
  onSwitchEntry,
  onView,
  onEdit,
  onEditFocusIntent,
  onEditHoverIntent,
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
  /** Fired when the Edit control takes focus — a deliberate, ungated signal. */
  onEditFocusIntent: () => void;
  /** Fired on pointer-enter of the Edit control; the caller decides whether to act. */
  onEditHoverIntent: () => void;
  onRemove: (e: TenantModelEntry) => void;
}) {
  if (row.kind === "default") {
    if (isDefaultLive) return null;
    return (
      <div className="flex flex-col items-end gap-1">
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={pending || !platformDefaultAvailable}
          onClick={onSwitchDefault}
          aria-label="Use default"
        >
          <ArrowLeftRightIcon size={14} aria-hidden="true" />Use default
        </Button>
        {!platformDefaultAvailable ? <span className="text-xs text-muted-foreground">{PLATFORM_UNAVAILABLE_NOTE}</span> : null}
      </div>
    );
  }
  const { entry } = row;
  return (
    <div className="flex items-center justify-end gap-1">
      <IconAction
        type="button"
        variant="ghost"
        disabled={pending}
        onClick={() => onEdit(entry)}
        // The Edit dialog's model picker needs the global catalogue. Warming
        // it on intent means the dialog opens populated instead of opening
        // empty and filling in underneath the user's cursor. Focus always
        // qualifies; hover only where it is a real signal — the caller owns
        // that policy, this just reports the gesture.
        onFocus={onEditFocusIntent}
        onPointerEnter={onEditHoverIntent}
        label={`Edit ${entry.model_id}`}
      >
        <PencilIcon size={14} />
      </IconAction>
      <IconAction
        type="button"
        variant="ghost"
        disabled={pending}
        onClick={() => onView(entry)}
        label={`View details for ${entry.model_id}`}
      >
        <EyeIcon size={14} />
      </IconAction>
      {!entry.active ? (
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={pending}
          onClick={() => onSwitchEntry(entry)}
          aria-label={`Switch to ${entry.model_id}`}
        >
          <ArrowLeftRightIcon size={14} aria-hidden="true" />Use model
        </Button>
      ) : null}
      <IconAction
        type="button"
        variant="destructive"
        disabled={pending || entry.active}
        onClick={() => onRemove(entry)}
        label={entry.active ? `Cannot remove ${entry.model_id} while it is active` : `Remove ${entry.model_id}`}
      >
        <Trash2Icon size={14} />
      </IconAction>
    </div>
  );
}
