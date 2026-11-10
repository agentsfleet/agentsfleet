import { Effect } from "effect";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import { Output, OUTPUT_FORMAT } from "../../services/output.ts";
import { ADMIN_MODELS_PATH, decodePlatform, invalidInput, platformInput, platformPath, platformQuery, platformRequest, platformResult, readPlatformFile } from "./common.ts";
import { PlatformModelCreateSchema, PlatformModelPageSchema, PlatformModelSchema, PlatformModelSettingsSchema, PlatformModelUpdatedSchema } from "./model-schema.ts";
import type { RunnerPaging } from "./runners.ts";

const MAX_CATALOGUE_PAGES = 1000;

export const platformModelsList = Effect.fn("platform.models.list")(function* (options: RunnerPaging) {
  const response = yield* modelPage(options);
  const output = yield* Output;
  if (output.format === OUTPUT_FORMAT.json) return yield* output.printJson(response);
  yield* output.printEntityTable({name: {key: "name", label: "NAME"}, domain: [
    {key: "id", label: "ID"}, {key: "provider", label: "PROVIDER"}, {key: "model_id", label: "MODEL"},
  ], ageKey: null}, response.models.map(({id, provider, model_id, metadata}) => ({id, provider, model_id, name: metadata.display_name})));
  if (response.next_cursor) yield* output.info(`Next page: --starting-after ${response.next_cursor}`);
});

export const platformModelShow = Effect.fn("platform.models.show")(function* (id: string) {
  let startingAfter: string | undefined;
  const seen = new Set<string>();
  for (let page = 0; page < MAX_CATALOGUE_PAGES; page += 1) {
    const response = yield* modelPage({startingAfter});
    const model = response.models.find((row) => row.id === id);
    if (model) return yield* platformResult(model);
    if (response.next_cursor === null) return yield* Effect.fail(invalidInput("This model was not found in the platform catalogue"));
    if (seen.has(response.next_cursor)) break;
    seen.add(response.next_cursor); startingAfter = response.next_cursor;
  }
  return yield* Effect.fail(invalidInput("The platform catalogue could not be read completely"));
});

export const platformModelWrite = Effect.fn("platform.models.write")(function* (file: string, id?: string) {
  const input = yield* readPlatformFile(file);
  if (id === undefined) {
    const body = yield* platformInput(PlatformModelCreateSchema, input);
    yield* platformResult(yield* decodePlatform(PlatformModelSchema, yield* platformRequest({path: ADMIN_MODELS_PATH, method: HTTP_METHOD.post, body})));
  } else {
    const body = yield* platformInput(PlatformModelSettingsSchema, input);
    yield* platformResult(yield* decodePlatform(PlatformModelUpdatedSchema, yield* platformRequest({path: platformPath(ADMIN_MODELS_PATH, id), method: HTTP_METHOD.patch, body})));
  }
});

export const platformModelDelete = Effect.fn("platform.models.delete")(function* (id: string) {
  yield* platformRequest({path: platformPath(ADMIN_MODELS_PATH, id), method: HTTP_METHOD.delete});
  yield* platformResult({id, deleted: true});
});

const modelPage = Effect.fn("platform.models.page")(function* (options: RunnerPaging) {
  return yield* decodePlatform(PlatformModelPageSchema, yield* platformRequest({
    path: ADMIN_MODELS_PATH + platformQuery({limit: options.limit, starting_after: options.startingAfter}),
  }));
});
