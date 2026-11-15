// A page remains explicit so a large Plugin never becomes a silent partial list.

import type { HttpClientShape } from "../services/http-client.ts";
import { Effect, Schema, type Redacted } from "effect";
import { instant } from "../lib/instant.ts";
import { isValidId } from "../lib/id.ts";
import { ServerError } from "../errors/index.ts";
import { QUERY_LIMIT, QUERY_STARTING_AFTER } from "../lib/api-paths.ts";

export interface PluginPageOptions {
  readonly startingAfter?: string | undefined;
  readonly limit?: string | undefined;
}

export const PLUGIN_PAGE_LIMIT = 100;
const object = Schema.Record(Schema.String, Schema.Unknown);
const uuid = Schema.String.check(Schema.makeFilter(isValidId));
export const pluginId = uuid.pipe(Schema.brand("PluginId"));
export const pluginRevisionId = uuid.pipe(Schema.brand("PluginRevisionId"));
export const pluginCard = Schema.Struct({
  id: pluginId,
  name: Schema.String,
  description: Schema.String,
  created_at: instant,
  requirements: object,
  required_credentials_reasons: Schema.Record(Schema.String, Schema.String),
  head_revision_id: pluginRevisionId,
  etag: Schema.String,
});
const page = Schema.Struct({
  items: Schema.Array(pluginCard).check(Schema.isMaxLength(PLUGIN_PAGE_LIMIT)),
  next_cursor: Schema.NullOr(Schema.String),
});

export const readPluginPage = Effect.fn("plugin.readPage")(function* (
  http: HttpClientShape,
  path: string,
  token: Redacted.Redacted<string>,
  options: PluginPageOptions,
) {
  const query = new URLSearchParams({ [QUERY_LIMIT]: options.limit ?? String(PLUGIN_PAGE_LIMIT) });
  if (options.startingAfter !== undefined) query.set(QUERY_STARTING_AFTER, options.startingAfter);
  const body = yield* http.request<unknown>({ path: `${path}?${query}`, token });
  return yield* decodePlugin(page, body);
});

export function decodePlugin<T>(schema: Schema.Codec<T, unknown>, body: unknown) {
  return Schema.decodeUnknownEffect(schema)(body).pipe(Effect.mapError(() => new ServerError({
    detail: "The plugin response did not match the supported API shape",
    suggestion: "verify that the terminal client targets the matching daemon build",
    code: "PLUGIN_RESPONSE_INVALID", status: 502, requestId: null,
  })));
}
