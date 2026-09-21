// One cursor walk over the daemon's `limit` / `starting_after` pagination.
//
// Twelve commands hand-rolled this loop, and two of them walked the SAME
// endpoint: `fleet_library` collected every gallery page while `fleet_install`
// scanned for one entry, each with its own `GALLERY_PAGE_LIMIT = 100` and
// `GALLERY_MAX_PAGES = 50`, and each spelling the query keys differently —
// `fleet_install` inlined "limit" and "starting_after" while `fleet_library`
// imported the constants. A comment in `fleet_library` read "Same page size
// and ceiling `install` resolves against. They must page alike", which is a
// comment doing a shared function's job: nothing but that sentence kept
// "what I can see" and "what I can install" the same set.
//
// The ceiling is a real guard, not a formality. A daemon that returns a
// `next_cursor` forever would otherwise spin a command until the operator
// kills it, so a walk that reaches PAGE_CEILING stops and says so.

import { Effect, type Redacted } from "effect";

import { QUERY_LIMIT, QUERY_STARTING_AFTER } from "./api-paths.ts";
import { HTTP_METHOD } from "../constants/http-method.ts";
import type { HttpClientShape } from "../services/http-client.ts";
import type { NetworkError, ServerError } from "../errors/index.ts";

/** One page as every paginated endpoint on this daemon answers it. */
export interface Page<T> {
  readonly items?: ReadonlyArray<T>;
  readonly next_cursor?: string | null;
}

/** Rows per request. The daemon's own maximum; a smaller number only costs
 *  round trips. */
export const PAGE_LIMIT = 100;

/** How many pages one walk will fetch before it refuses to keep going. */
export const PAGE_CEILING = 50;

/** The query string for one page of a walk. */
const pageQuery = (cursor: string | null): string => {
  const params = new URLSearchParams({ [QUERY_LIMIT]: String(PAGE_LIMIT) });
  if (cursor !== null) params.set(QUERY_STARTING_AFTER, cursor);
  return params.toString();
};

const pageUrl = (path: string, cursor: string | null): string =>
  `${path}${path.includes("?") ? "&" : "?"}${pageQuery(cursor)}`;

/**
 * Every row the walk reaches, in server order.
 *
 * Stops at [`PAGE_CEILING`] and returns what it has: a partial gallery renders
 * something an operator can act on, where a thrown error at page fifty would
 * discard forty-nine pages of good rows.
 */
export const collectPages = <T>(
  http: HttpClientShape,
  path: string,
  token: Redacted.Redacted<string>,
): Effect.Effect<T[], NetworkError | ServerError> =>
  Effect.gen(function* () {
    const rows: T[] = [];
    let cursor: string | null = null;
    for (let page = 0; page < PAGE_CEILING; page += 1) {
      const body: Page<T> = yield* http.request<Page<T>>({
        path: pageUrl(path, cursor),
        method: HTTP_METHOD.get,
        token,
      });
      rows.push(...(body.items ?? []));
      if (!body.next_cursor) break;
      cursor = body.next_cursor;
    }
    return rows;
  });

/**
 * The first row satisfying `match`, or `undefined` once the walk runs out.
 *
 * Short-circuits: a hit on page one issues one request. That is the whole
 * reason this is not `collectPages(...).find(...)` — `install` resolving a
 * library id should not pull the entire gallery to look at one row.
 */
export const findAcrossPages = <T>(
  http: HttpClientShape,
  path: string,
  token: Redacted.Redacted<string>,
  match: (row: T) => boolean,
): Effect.Effect<T | undefined, NetworkError | ServerError> =>
  Effect.gen(function* () {
    let cursor: string | null = null;
    for (let page = 0; page < PAGE_CEILING; page += 1) {
      const body: Page<T> = yield* http.request<Page<T>>({
        path: pageUrl(path, cursor),
        method: HTTP_METHOD.get,
        token,
      });
      const hit = (body.items ?? []).find(match);
      if (hit !== undefined) return hit;
      if (!body.next_cursor) return undefined;
      cursor = body.next_cursor;
    }
    return undefined;
  });
