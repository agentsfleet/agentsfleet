import { STATUS_TONES } from "../../output/format.ts";
import { Effect, Schema } from "effect";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import { Output, OUTPUT_FORMAT } from "../../services/output.ts";
import { instant } from "../../lib/instant.ts";
import { ADMIN_KEYS_PATH, decodePlatform, platformPath, platformRequest, platformResult } from "./common.ts";

type DefaultOptions = {readonly provider: string; readonly workspace: string; readonly model: string; readonly baseUrl?: string | undefined};
const key = Schema.Struct({provider: Schema.String, source_workspace_id: Schema.String, model: Schema.NullOr(Schema.String), active: Schema.Boolean, updated_at: instant});
const keys = Schema.Struct({keys: Schema.Array(key)});
const selected = Schema.Struct({provider: Schema.String, model: Schema.String, active: Schema.Boolean});
const DEFAULT_STATUS = "Default";
const status = (active: boolean): string => active ? DEFAULT_STATUS : "Configured";

export const platformDefaultList = Effect.fn("platform.models.default.list")(function* () {
  const response = yield* decodePlatform(keys, yield* platformRequest({path: ADMIN_KEYS_PATH}));
  const output = yield* Output;
  if (output.format === OUTPUT_FORMAT.json || !output.stdoutIsTty) return yield* output.printJson(response);
  yield* output.printEntityTable({id: {key: "provider"}, domain: [
    {key: "model", label: "MODEL"}, {key: "source_workspace_id", label: "WORKSPACE"},
    {key: "status", label: "STATUS", tones: STATUS_TONES},
  ], ageKey: null}, response.keys.map(row => ({...row, status: status(row.active)})));
});

export const platformDefaultUse = Effect.fn("platform.models.default.use")(function* (options: DefaultOptions) {
  const body = {provider: options.provider, source_workspace_id: options.workspace, model: options.model,
    ...options.baseUrl === undefined ? {} : {base_url: options.baseUrl}};
  const result = yield* decodePlatform(selected, yield* platformRequest({path: ADMIN_KEYS_PATH, method: HTTP_METHOD.put, body}));
  yield* platformResult(result, "Platform model", {Provider: result.provider, Model: result.model, Status: status(result.active)}, {Status: STATUS_TONES});
});

export const platformDefaultDelete = Effect.fn("platform.models.default.delete")(function* (provider: string) {
  yield* platformRequest({path: platformPath(ADMIN_KEYS_PATH, provider), method: HTTP_METHOD.delete});
  yield* platformResult({provider, deleted: true}, "Platform default deleted", {Provider: provider});
});
