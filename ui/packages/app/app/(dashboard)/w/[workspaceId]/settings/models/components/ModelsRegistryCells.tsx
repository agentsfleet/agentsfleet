"use client";

import { Badge, IconAction } from "@agentsfleet/design-system";
import { ArrowLeftRightIcon, EyeIcon, LockIcon, PencilIcon, Trash2Icon } from "lucide-react";
import { type LibraryModel, providerLabel } from "@/lib/api/model_library";
import { nanosToUsdPerMtok } from "@/lib/api/admin_model_library";
import type { TenantModelEntry, TenantPlatformDefault } from "@/lib/types";

// Presentational cells + row actions for ModelsRegistryTable, split out so the
// stateful table module stays under the file-length cap. Everything here is
// pure props → markup; state and server actions live in the table.

export type RegistryRow = { kind: "default" } | { kind: "entry"; entry: TenantModelEntry };

const PLATFORM_UNAVAILABLE_NOTE = "No default is configured.";
// Threshold + divisor for the "k" context abbreviation (200000 → "200k").
const TOKENS_PER_K = 1000;
const EMPTY_VALUE = "—";
const RATES_UNAVAILABLE = "Rates unavailable";

export function rowKey(row: RegistryRow): string {
  return row.kind === "default" ? "default" : row.entry.id;
}

// `context_cap_tokens` is a Zig `?u32` on the wire (schema/embed.zig) — the
// only real-world absent case is "not in the catalogue" (undefined). Guard
// on nullishness, not falsiness, so a (semantically invalid but not
// impossible) explicit 0 still renders as "0" rather than "—".
export function formatContext(tokens: number | undefined): string {
  if (tokens == null) return EMPTY_VALUE;
  return tokens >= TOKENS_PER_K ? `${Math.round(tokens / TOKENS_PER_K)}k` : String(tokens);
}

type RateFields = Pick<LibraryModel, "input_nanos_per_mtok" | "cached_input_nanos_per_mtok" | "output_nanos_per_mtok">;

/** The library row pricing (provider, model_id) — null when the library
 * doesn't price it (custom endpoints, or the library fetch failed). */
export function libraryRateFor(
  models: LibraryModel[],
  provider: string | undefined,
  modelId: string,
): LibraryModel | null {
  if (!provider) return null;
  return models.find((m) => m.provider === provider && m.id === modelId) ?? null;
}

function rowRateFor(row: TenantModelEntry | TenantPlatformDefault): RateFields | null {
  if (
    row.input_nanos_per_mtok == null ||
    row.cached_input_nanos_per_mtok == null ||
    row.output_nanos_per_mtok == null
  ) {
    return null;
  }
  return {
    input_nanos_per_mtok: row.input_nanos_per_mtok,
    cached_input_nanos_per_mtok: row.cached_input_nanos_per_mtok,
    output_nanos_per_mtok: row.output_nanos_per_mtok,
  };
}

/** "in / cached / out" per-1M line — the admin catalogue's presentation; the
 * column header carries the "$/1M" unit so the cell stays compact. */
export function formatRates(rate: RateFields | null): string {
  if (!rate) return RATES_UNAVAILABLE;
  const usd = (nanos: number) => nanosToUsdPerMtok(nanos).toFixed(2);
  return `${usd(rate.input_nanos_per_mtok)} / ${usd(rate.cached_input_nanos_per_mtok)} / ${usd(rate.output_nanos_per_mtok)}`;
}

export function ModelCell({
  row,
  platformDefault,
}: {
  row: RegistryRow;
  platformDefault: TenantPlatformDefault | null;
}) {
  if (row.kind === "default") {
    return (
      <span className="inline-flex min-w-0 items-center gap-2">
        <span>Default</span>
        <LockIcon size={12} className="shrink-0 text-muted-foreground" aria-label="Managed by a platform admin" />
        {platformDefault ? (
          <span className="truncate font-mono text-sm text-muted-foreground">{platformDefault.model}</span>
        ) : null}
      </span>
    );
  }
  return <span className="truncate font-mono text-sm">{row.entry.model_id}</span>;
}

export function ProviderCell({
  row,
  platformDefault,
}: {
  row: RegistryRow;
  platformDefault: TenantPlatformDefault | null;
}) {
  if (row.kind === "default") {
    return (
      <div className="min-w-0">
        {platformDefault ? <div className="text-sm">{providerLabel(platformDefault.provider)}</div> : null}
        <div className="text-xs text-muted-foreground">Platform-managed</div>
      </div>
    );
  }
  const { entry } = row;
  return (
    <div className="min-w-0">
      <div className="text-sm">{entry.provider ? providerLabel(entry.provider) : "Unknown"}</div>
      {entry.base_url ? <div className="truncate font-mono text-xs text-muted-foreground">{entry.base_url}</div> : null}
    </div>
  );
}

/** Context cap over the library's per-token rates — the Default row reads
 * both from the ridden-along platform default identity. */
export function ContextCell({
  row,
  platformDefault,
  libraryModels,
}: {
  row: RegistryRow;
  platformDefault: TenantPlatformDefault | null;
  libraryModels: LibraryModel[];
}) {
  const identity =
    row.kind === "default"
      ? platformDefault && {
          provider: platformDefault.provider,
          model: platformDefault.model,
          context: platformDefault.context_cap_tokens,
          rate: rowRateFor(platformDefault),
        }
      : {
          provider: row.entry.provider,
          model: row.entry.model_id,
          context: row.entry.context_cap_tokens,
          rate: rowRateFor(row.entry),
        };
  if (!identity) {
    return <span className="font-mono text-xs tabular-nums text-muted-foreground">{EMPTY_VALUE}</span>;
  }
  const rate = identity.rate ?? libraryRateFor(libraryModels, identity.provider, identity.model);
  return (
    <div className="font-mono text-xs tabular-nums text-muted-foreground">
      <div>{formatContext(identity.context)}</div>
      <div>{formatRates(rate)}</div>
    </div>
  );
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
  onRemove: (e: TenantModelEntry) => void;
}) {
  if (row.kind === "default") {
    if (isDefaultLive) return null;
    return (
      <div className="flex flex-col items-end gap-1">
        <IconAction
          type="button"
          variant="ghost"
          disabled={pending || !platformDefaultAvailable}
          onClick={onSwitchDefault}
          label="Use default"
        >
          <ArrowLeftRightIcon size={14} />
        </IconAction>
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
        onClick={() => onView(entry)}
        label={`View details for ${entry.model_id}`}
      >
        <EyeIcon size={14} />
      </IconAction>
      {!entry.active ? (
        <IconAction
          type="button"
          variant="ghost"
          disabled={pending}
          onClick={() => onSwitchEntry(entry)}
          label={`Switch to ${entry.model_id}`}
        >
          <ArrowLeftRightIcon size={14} />
        </IconAction>
      ) : null}
      <IconAction
        type="button"
        variant="ghost"
        disabled={pending}
        onClick={() => onEdit(entry)}
        label={`Edit ${entry.model_id}`}
      >
        <PencilIcon size={14} />
      </IconAction>
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
