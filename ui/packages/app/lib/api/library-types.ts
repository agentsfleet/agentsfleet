// Shared page/error shapes for the two authenticated library surfaces: the
// tenant Models registry (`tenant_model_entries.ts`) and the Fleet gallery
// (`fleet-library.ts`). Both consume the same keyset endpoints, so both carry
// the same cursor, retention, and failure vocabulary — kept here rather than
// duplicated so a state a reducer forgets to handle fails to compile in both.
//
// No `cache()` and no server-only import: these are plain types shared by
// server reads and client reducers alike.

/**
 * Load state for a retained paged list.
 *
 * `refreshing` and `error` both carry `items` because a revalidation fault must
 * leave the last successful rows on screen. Collapsing either into an empty
 * state is the failure this workstream exists to remove — see the spec's
 * `test_refresh_retains_authorized_content`.
 */
export const LIBRARY_LOAD_STATUS = {
  idle: "idle",
  loading: "loading",
  refreshing: "refreshing",
  ready: "ready",
  error: "error",
} as const;

export type LibraryLoadStatus =
  (typeof LIBRARY_LOAD_STATUS)[keyof typeof LIBRARY_LOAD_STATUS];

/**
 * Typed read failure. `notFound` is a *selection* outcome rather than a
 * transport one: the workspace detail route was retired, so an unknown
 * `library_id` is an id absent from the gallery, not a 404 from a fetch.
 */
export const LIBRARY_ERROR_KIND = {
  unauthenticated: "unauthenticated",
  forbidden: "forbidden",
  notFound: "notFound",
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
 * One retained page window. `nextCursor` null means the server said this is the
 * last page; `retained` is every row loaded so far, not just the newest page.
 */
export type LibraryPage<TItem> = {
  retained: TItem[];
  nextCursor: string | null;
  /**
   * Server-reported total when the endpoint supplies one, else null.
   *
   * Invariant 5: a paged list must disclose what it has NOT loaded. Both
   * surfaces previously walked `next_cursor` to exhaustion precisely so later
   * entries could not vanish unannounced; paging reintroduces that hazard, so
   * `retained.length` and `hasMore` are rendered rather than implied by whether
   * a button happens to be present. A null `total` still discloses via
   * `hasMore` — it just cannot name the remainder.
   */
  total: number | null;
};

export type LibraryListState<TItem> =
  | { status: typeof LIBRARY_LOAD_STATUS.idle }
  | { status: typeof LIBRARY_LOAD_STATUS.loading }
  | ({ status: typeof LIBRARY_LOAD_STATUS.ready } & LibraryPage<TItem>)
  | ({ status: typeof LIBRARY_LOAD_STATUS.refreshing } & LibraryPage<TItem>)
  | ({ status: typeof LIBRARY_LOAD_STATUS.error; error: LibraryError } & Partial<
      LibraryPage<TItem>
    >);

/** True while the list holds rows worth rendering, whatever else is happening. */
export function hasRetainedItems<TItem>(
  state: LibraryListState<TItem>,
): state is Extract<
  LibraryListState<TItem>,
  { retained?: TItem[] }
> & { retained: TItem[] } {
  return "retained" in state && Array.isArray(state.retained) && state.retained.length > 0;
}

/** More pages remain on the server. Drives the load-more affordance. */
export function hasMore<TItem>(state: LibraryListState<TItem>): boolean {
  return "nextCursor" in state && state.nextCursor !== null;
}

/**
 * Map a transport status onto the typed vocabulary. 404 is deliberately absent:
 * no read path in this workstream produces one, and mapping it would invent a
 * state no server emits.
 */
export function errorKindForStatus(status: number): LibraryErrorKind {
  if (status === 401) return LIBRARY_ERROR_KIND.unauthenticated;
  if (status === 403) return LIBRARY_ERROR_KIND.forbidden;
  if (status === 503) return LIBRARY_ERROR_KIND.unavailable;
  return LIBRARY_ERROR_KIND.unknown;
}
