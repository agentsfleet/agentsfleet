// Read failures for the authenticated Models registry.
// Fleet Plugin reads use the authoring transport vocabulary.
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
  const status = cause !== null && typeof cause === "object" && "status" in cause ? cause.status : undefined;
  return {
    kind: typeof status === "number" ? errorKindForStatus(status) : LIBRARY_ERROR_KIND.unknown,
    detail: cause instanceof Error ? cause.message : undefined,
  };
}
