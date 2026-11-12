// Pure view helpers for the Models registry table — split out of
// ModelsRegistryTable.tsx along its stateless seam when that file crossed the
// length cap. Everything here is a pure function over passive values; the
// component keeps the state and wiring.

import { LIBRARY_ERROR_KIND, type LibraryError } from "@/lib/api/library-types";
import { type LibraryModel } from "@/lib/api/model-library-types";
import type { TenantModelEntry, TenantPlatformDefault } from "@/lib/types";
import { type RateFields } from "@/lib/models/display";

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
