// Pure view helpers for the Models registry table — split out of
// ModelsRegistryTable.tsx along its stateless seam when that file crossed the
// length cap. Everything here is a pure function over passive values; the
// component keeps the state and wiring.

import { LIBRARY_ERROR_KIND, type LibraryError } from "@/lib/api/library-types";
import type { LibraryModel } from "@/lib/api/model-library-types";
import type { TenantModelEntry, TenantPlatformDefault } from "@/lib/types";
import { compareModelIds, type RateFields } from "@/lib/models/display";

/** The columns the table sorts, matching the model library's set. */
const SORT_KEYS = ["model", "provider", "context", "rates"] as const;
export type SortKey = (typeof SORT_KEYS)[number];
export type SortState = { key: SortKey; dir: "ascending" | "descending" } | null;

/** An entry with no figure sorts before every real one when ascending, as
 * Events does. */
const ABSENT_FIGURE = -1;

function isSortKey(key: string): key is SortKey {
  return (SORT_KEYS as readonly string[]).includes(key);
}

// Pure — DataTable's onSortChange prop is typed `(key: string) => void` (any
// column could be sortable), but only the SORT_KEYS columns opt in;
// a `key` outside that set returns `null` (no-op) instead of ever reaching
// component state. Exported so the boundary is unit-testable without needing
// to reach it through a real DataTable header click.
export function computeNextSort(cur: SortState, key: string): SortState | null {
  if (!isSortKey(key)) return null;
  if (!cur || cur.key !== key) return { key, dir: "ascending" };
  return { key, dir: cur.dir === "ascending" ? "descending" : "ascending" };
}

/** Pure — the order two entries take under a sort column. The model column
 * orders exactly as the model library does, through the one shared comparison;
 * rates order by the input rate the Rates cell shows, the entry's own or else
 * the library's. */
export function compareEntries(
  a: TenantModelEntry,
  b: TenantModelEntry,
  key: SortKey,
  libraryModels: LibraryModel[] = [],
): number {
  switch (key) {
    case "model":
      return compareModelIds(a.model_id, b.model_id);
    case "provider":
      return (a.provider ?? "").localeCompare(b.provider ?? "");
    case "context":
      return (a.context_cap_tokens ?? ABSENT_FIGURE) - (b.context_cap_tokens ?? ABSENT_FIGURE);
    case "rates":
      return inputRateOf(a, libraryModels) - inputRateOf(b, libraryModels);
  }
}

/** The input rate an entry's Rates cell shows, or ABSENT_FIGURE. */
function inputRateOf(entry: TenantModelEntry, libraryModels: LibraryModel[]): number {
  const rate = rowRateFor(entry) ?? libraryRateFor(libraryModels, entry.provider, entry.model_id);
  return rate?.input_nanos_per_mtok ?? ABSENT_FIGURE;
}

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

export function rowRateFor(row: TenantModelEntry | TenantPlatformDefault): RateFields | null {
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

/**
 * Pure — user-facing copy for a typed read failure. Each kind gets its own
 * next step, which is the whole point of typing them: "sign in" and "ask for
 * access" and "try again" are different instructions, and an empty table gave
 * the user none of them.
 */
export function readErrorCopy(error: LibraryError): string {
  switch (error.kind) {
    case LIBRARY_ERROR_KIND.unauthenticated:
      return "Your session expired. Sign in to see your models.";
    case LIBRARY_ERROR_KIND.forbidden:
      return "You do not have access to this workspace's models.";
    case LIBRARY_ERROR_KIND.unavailable:
      return "Models are temporarily unavailable. Your entries are safe.";
    default:
      return "Could not load your models. They have not been changed.";
  }
}
