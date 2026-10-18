// A page remains explicit so a large Library never becomes a silent partial list.

import type { HttpClientShape } from "../services/http-client.ts";
import { Effect, Schema, type Redacted } from "effect";
import { instant } from "../lib/instant.ts";
import { ServerError } from "../errors/index.ts";
import { QUERY_LIMIT, QUERY_STARTING_AFTER } from "../lib/api-paths.ts";

export interface LibraryPageOptions {
  readonly startingAfter?: string | undefined;
  readonly limit?: string | undefined;
}

export const LIBRARY_PAGE_LIMIT = 100;
export const LIBRARY_VISIBILITY = { platform: "platform", tenant: "tenant" } as const;
const object = Schema.Record(Schema.String, Schema.Unknown);
const card = Schema.Struct({
  id: Schema.String,
  name: Schema.String,
  description: Schema.String,
  visibility: Schema.Literals(Object.values(LIBRARY_VISIBILITY)),
  created_at: instant,
  requirements: object,
  required_credentials_reasons: Schema.Record(Schema.String, Schema.String),
  head_revision_id: Schema.String,
  etag: Schema.String,
});
const page = Schema.Struct({
  items: Schema.Array(card).check(Schema.isMaxLength(LIBRARY_PAGE_LIMIT)),
  next_cursor: Schema.NullOr(Schema.String),
});

export const readLibraryPage = Effect.fn("library.readPage")(function* (
  http: HttpClientShape,
  path: string,
  token: Redacted.Redacted<string>,
  options: LibraryPageOptions,
) {
  const query = new URLSearchParams({ [QUERY_LIMIT]: options.limit ?? String(LIBRARY_PAGE_LIMIT) });
  if (options.startingAfter !== undefined) query.set(QUERY_STARTING_AFTER, options.startingAfter);
  const body = yield* http.request<unknown>({ path: `${path}?${query}`, token });
  return yield* Schema.decodeUnknownEffect(page)(body).pipe(Effect.mapError(() => new ServerError({
    detail: "The Library response did not match the supported API shape",
    suggestion: "verify that the terminal client targets the matching daemon build",
    code: "LIBRARY_RESPONSE_INVALID", status: 502, requestId: null,
  })));
});
