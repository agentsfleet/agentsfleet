// Shared failure vocabulary for the two authenticated library surfaces: the
// tenant Models registry (`tenant_model_entries.ts`) and the Fleet gallery
// (`fleet-library.ts`). Both consume the same keyset endpoints, so both carry
// the same typed-failure shapes — kept here rather than duplicated so the two
// surfaces cannot drift.
//
// No `cache()` and no server-only import: these are plain types and pure
// functions shared by server reads and client components alike.

/**
 * Typed read failure. A deep-linked id absent from the loaded page is NOT a
 * kind here — it is a *selection* outcome (`selectionNotFound` on the install
 * screen), because no read path in this vocabulary's surfaces produces a 404.
 */
export const LIBRARY_ERROR_KIND = {
  unauthenticated: "unauthenticated",
  forbidden: "forbidden",
  unavailable: "unavailable",
  unknown: "unknown",
} as const;

export type LibraryErrorKind =
  (typeof LIBRARY_ERROR_KIND)[keyof typeof LIBRARY_ERROR_KIND];

export type LibraryError = {
  kind: LibraryErrorKind;
  /** Operator-facing detail. Never rendered as the sole user-facing copy. */
  detail?: string;
};

/**
 * The query parameter carrying gallery list position. Written by the install
 * screen's load-more URL mirror and parsed by its server render — one name,
 * because a producer/parser drift compiles clean and silently breaks the
 * position restore.
 */
export const LIBRARY_AFTER_PARAM = "library_after";

/**
 * Map a transport status onto the typed vocabulary. 404 is deliberately
 * absent: no read path in this vocabulary's surfaces produces one, and
 * mapping it would invent a state no server emits.
 */
export function errorKindForStatus(status: number): LibraryErrorKind {
  if (status === 401) return LIBRARY_ERROR_KIND.unauthenticated;
  if (status === 403) return LIBRARY_ERROR_KIND.forbidden;
  if (status === 503) return LIBRARY_ERROR_KIND.unavailable;
  return LIBRARY_ERROR_KIND.unknown;
}

/**
 * Pure — an ActionResult failure mapped onto the typed vocabulary. The action
 * layer preserves the transport status when it has one, which is what lets a
 * 401/403/503 keep its specific copy instead of collapsing to "unknown".
 */
export function readErrorFrom(failure: { error: string; status?: number }): LibraryError {
  return {
    kind: typeof failure.status === "number" ? errorKindForStatus(failure.status) : LIBRARY_ERROR_KIND.unknown,
    detail: failure.error,
  };
}

/**
 * Pure — a thrown/rejected read mapped onto the typed vocabulary. `ApiError`
 * carries `status`; anything else (network failure, deploy skew) is unknown.
 */
export function libraryErrorFromCause(cause: unknown): LibraryError {
  const status = (cause as { status?: number }).status;
  return {
    kind: typeof status === "number" ? errorKindForStatus(status) : LIBRARY_ERROR_KIND.unknown,
    detail: cause instanceof Error ? cause.message : undefined,
  };
}
