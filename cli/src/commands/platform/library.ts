import { Effect, Schema } from "effect";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import { instant } from "../../lib/instant.ts";
import { PublishedSchema } from "../chat/protocol.ts";
import { ADMIN_LIBRARY_PATH, decodePlatform, invalidInput, platformInput, platformPath, platformRequest, platformResult, readPlatformFile } from "./common.ts";

const text = Schema.String.check(Schema.isMinLength(1));
const record = Schema.Record(Schema.String, Schema.String);
const card = Schema.Struct({id: text, name: text, description: Schema.String, visibility: Schema.Literals(["platform", "platform_draft"]),
  head_revision_id: text, etag: text, requirements: Schema.Record(Schema.String, Schema.Unknown), created_at: instant, required_credentials_reasons: record});
const entry = Schema.Struct({card, published: Schema.Boolean, updated_at: instant});
const entries = Schema.Struct({entries: Schema.Array(entry)});
const metadata = Schema.Struct({name: Schema.optionalKey(text), description: Schema.optionalKey(Schema.String), required_credentials_reasons: Schema.optionalKey(record)});

export const platformLibraryList = Effect.fn("platform.library.list")(function* (id?: string) {
  const result = yield* decodePlatform(entries, yield* platformRequest({path: ADMIN_LIBRARY_PATH}));
  if (id === undefined) return yield* platformResult(result);
  const found = result.entries.find((item) => item.card.id === id);
  if (!found) return yield* Effect.fail(invalidInput("This platform library entry was not found"));
  yield* platformResult(found);
});

export const platformLibraryStage = Effect.fn("platform.library.stage")(function* (workspace: string, candidate: string, label: string, operation: string) {
  const result = yield* platformRequest({path: ADMIN_LIBRARY_PATH, method: HTTP_METHOD.post,
    headers: {"Idempotency-Key": operation}, body: {workspace_id: workspace, draft_candidate_id: candidate, label}});
  yield* platformResult(yield* decodePlatform(PublishedSchema, result));
});

export const platformLibraryUpdate = Effect.fn("platform.library.update")(function* (id: string, file: string, etag: string) {
  const body = yield* platformInput(metadata, yield* readPlatformFile(file));
  if (Object.keys(body).length === 0) return yield* Effect.fail(invalidInput("Supply at least one library metadata field"));
  yield* mutateLibrary(id, body, etag);
});

export const platformLibraryPublish = Effect.fn("platform.library.publish")(function* (id: string, published: boolean, etag: string) {
  yield* mutateLibrary(id, {published}, etag);
});

export const platformLibraryDelete = Effect.fn("platform.library.delete")(function* (id: string, etag: string) {
  yield* platformRequest({path: platformPath(ADMIN_LIBRARY_PATH, id), method: HTTP_METHOD.delete, headers: {"If-Match": etag}});
  yield* platformResult({id, deleted: true});
});

const mutateLibrary = Effect.fn("platform.library.mutate")(function* (id: string, body: unknown, etag: string) {
  const result = yield* platformRequest({path: platformPath(ADMIN_LIBRARY_PATH, id), method: HTTP_METHOD.patch, headers: {"If-Match": etag}, body});
  yield* platformResult(yield* decodePlatform(entry, result));
});
