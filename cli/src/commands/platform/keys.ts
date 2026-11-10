import { Effect, Schema } from "effect";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import { Stdin } from "../../services/stdin.ts";
import { wsSecretsPath } from "../../lib/api-paths.ts";
import { instant } from "../../lib/instant.ts";
import { ADMIN_KEYS_PATH, decodePlatform, invalidInput, platformPath, platformRequest, platformResult } from "./common.ts";

type KeyOptions = {readonly provider: string; readonly workspace: string; readonly model: string; readonly baseUrl?: string | undefined; readonly apiKeyStdin: boolean};
const key = Schema.Struct({provider: Schema.String, source_workspace_id: Schema.String, model: Schema.NullOr(Schema.String), active: Schema.Boolean, updated_at: instant});
const keys = Schema.Struct({keys: Schema.Array(key)});
const selected = Schema.Struct({provider: Schema.String, model: Schema.String, active: Schema.Boolean});
const STDIN_DETAIL = "Pipe the provider API key into stdin when using --api-key-stdin";
const KEY_MAX_BYTES = 64 * 1024;

export const platformKeysList = Effect.fn("platform.keys.list")(function* () {
  yield* platformResult(yield* decodePlatform(keys, yield* platformRequest({path: ADMIN_KEYS_PATH})));
});

export const platformKeysSet = Effect.fn("platform.keys.set")(function* (options: KeyOptions) {
  if (options.apiKeyStdin) {
    const stdin = yield* Stdin;
    if (stdin.isTTY) return yield* Effect.fail(invalidInput(STDIN_DETAIL));
    const apiKey = (yield* stdin.readToEnd).trim();
    if (!apiKey || Buffer.byteLength(apiKey) > KEY_MAX_BYTES) return yield* Effect.fail(invalidInput(STDIN_DETAIL));
    yield* platformRequest({path: wsSecretsPath(options.workspace), method: HTTP_METHOD.post,
      body: {name: options.provider, data: {provider: options.provider, api_key: apiKey, model: options.model,
        ...options.baseUrl === undefined ? {} : {base_url: options.baseUrl}}},
    });
  }
  const body = {provider: options.provider, source_workspace_id: options.workspace, model: options.model,
    ...options.baseUrl === undefined ? {} : {base_url: options.baseUrl}};
  yield* platformResult(yield* decodePlatform(selected, yield* platformRequest({path: ADMIN_KEYS_PATH, method: HTTP_METHOD.put, body})));
});

export const platformKeysDelete = Effect.fn("platform.keys.delete")(function* (provider: string) {
  yield* platformRequest({path: platformPath(ADMIN_KEYS_PATH, provider), method: HTTP_METHOD.delete});
  yield* platformResult({provider, deleted: true});
});
