// Plugin readers preserve the complete response for machine consumers.

import type { PluginPageOptions } from "./plugins-read.ts";
import { Effect, Schema } from "effect";
import { HttpClient } from "../services/http-client.ts";
import { Output, OUTPUT_FORMAT } from "../services/output.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace/guards.ts";
import { wsPluginPath, wsPluginRevisionsPath, wsPluginRevisionPath, QUERY_LIMIT, QUERY_STARTING_AFTER } from "../lib/api-paths.ts";
import { instant } from "../lib/instant.ts";
import { AGE_KEY, ui } from "../output/index.ts";
import { decodePlugin, pluginCard, pluginId, pluginRevisionId, PLUGIN_PAGE_LIMIT } from "./plugins-read.ts";

const decimal = Schema.String.check(Schema.isPattern(/^[1-9]\d*$/u));
const summary = Schema.Struct({id: pluginRevisionId, revision: decimal, canonical_digest: Schema.String, created_at: instant});
const history = Schema.Struct({items: Schema.Array(summary).check(Schema.isMaxLength(PLUGIN_PAGE_LIMIT)), next_cursor: Schema.NullOr(Schema.String)});
const show = Schema.Struct({card: pluginCard, etag: Schema.String});
const detail = Schema.Struct({revision: Schema.Struct({id: pluginRevisionId, plugin_id: pluginId, revision: decimal, canonical_digest: Schema.String}),
  review: Schema.Struct({review_digest: Schema.String})});

export const pluginShowEffect = Effect.fn("plugins.show")(function* (id: typeof pluginId.Type) {
  const output = yield* Output;
  const {body, value} = yield* request(workspace => wsPluginPath(workspace, id), show);
  if (output.format !== OUTPUT_FORMAT.text) return yield* output.printJson(body);
  yield* output.printSection(value.card.name);
  yield* output.printKeyValue({name: value.card.name, id: value.card.id, head_revision_id: value.card.head_revision_id, etag: value.etag});
});

export const pluginRevisionEffect = Effect.fn("plugins.revision")(function* (id: typeof pluginId.Type, revision: typeof pluginRevisionId.Type) {
  const output = yield* Output;
  const {body, value} = yield* request(workspace => wsPluginRevisionPath(workspace, id, revision), detail);
  if (output.format !== OUTPUT_FORMAT.text) return yield* output.printJson(body);
  yield* output.printSection(`Plugin revision ${value.revision.revision}`);
  yield* output.printKeyValue({revision: value.revision.revision, canonical_digest: value.revision.canonical_digest, review_digest: value.review.review_digest});
});

export const pluginHistoryEffect = Effect.fn("plugins.revisions")(function* (id: typeof pluginId.Type, options: PluginPageOptions) {
  const output = yield* Output;
  const query = new URLSearchParams({[QUERY_LIMIT]: options.limit ?? String(PLUGIN_PAGE_LIMIT)});
  if (options.startingAfter !== undefined) query.set(QUERY_STARTING_AFTER, options.startingAfter);
  const {body, value} = yield* request(workspace => `${wsPluginRevisionsPath(workspace, id)}?${query}`, history);
  if (output.format !== OUTPUT_FORMAT.text) return yield* output.printJson(body);
  if (value.items.length === 0) yield* output.info("No published revisions on this page.");
  else yield* output.printEntityTable({name: {key: "revision", label: "REVISION"}, id: {key: "id", label: "ID"},
    domain: [{key: "canonical_digest", label: "DIGEST"}, {key: "published_at", label: "CREATED"}]},
  value.items.map(item => ({...item, published_at: item.created_at, [AGE_KEY]: item.created_at})));
  if (value.next_cursor !== null) yield* output.info(ui.dim(
    `More available: agentsfleet plugins revisions ${id} --starting-after ${value.next_cursor}${options.limit ? ` --limit ${options.limit}` : ""}`,
  ));
});

const request = Effect.fn("plugins.inspect.request")(function* <T>(path: (workspace: string) => string, schema: Schema.Codec<T, unknown>) {
  const http = yield* HttpClient;
  const workspace = yield* requireWorkspaceId;
  const token = yield* resolveAuthToken;
  const body = yield* http.request<unknown>({path: path(workspace), token});
  const value = yield* decodePlugin(schema, body);
  return {body, value};
});
