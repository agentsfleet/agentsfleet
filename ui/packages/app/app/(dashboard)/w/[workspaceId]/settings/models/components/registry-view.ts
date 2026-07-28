// Pure view helpers for the Models registry table — split out of
// ModelsRegistryTable.tsx along its stateless seam when that file crossed the
// length cap. Everything here is a pure function over passive values; the
// component keeps the state and wiring.

import { LIBRARY_ERROR_KIND, type LibraryError } from "@/lib/api/library-types";
import type { TenantModelEntry } from "@/lib/types";

export type SortState = { key: "model" | "provider"; dir: "ascending" | "descending" } | null;

// Pure — DataTable's onSortChange prop is typed `(key: string) => void` (any
// column could be sortable), but only the "model"/"provider" columns opt in;
// a `key` outside that set returns `null` (no-op) instead of ever reaching
// component state. Exported so the boundary is unit-testable without needing
// to reach it through a real DataTable header click.
export function computeNextSort(cur: SortState, key: string): SortState | null {
  if (key !== "model" && key !== "provider") return null;
  if (!cur || cur.key !== key) return { key, dir: "ascending" };
  return { key, dir: cur.dir === "ascending" ? "descending" : "ascending" };
}

/** Pure — the sort comparator's per-row key, single call site per column. */
export function sortValueFor(entry: TenantModelEntry, key: "model" | "provider"): string {
  return key === "model" ? entry.model_id : (entry.provider ?? "");
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
