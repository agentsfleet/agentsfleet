import type { HttpRequestInput } from "../../services/http-client.ts";
import { Effect, Schema } from "effect";
import { HttpClient } from "../../services/http-client.ts";
import { Output } from "../../services/output.ts";
import { Stdin } from "../../services/stdin.ts";
import { UnexpectedError, ValidationError } from "../../errors/index.ts";
import { resolveAuthToken } from "../workspace-guards.ts";

export const ADMIN_RUNNERS_PATH = "/v1/admin/runners";
export const ADMIN_MODELS_PATH = "/v1/admin/models";
export const ADMIN_KEYS_PATH = "/v1/admin/platform-keys";
export const ADMIN_LIBRARY_PATH = "/v1/admin/fleet-libraries";
const INPUT_MAX_BYTES = 1024 * 1024;
const INPUT_HINT = "use the JSON shape documented by `agentsfleet platform --help`";
const RESPONSE_DETAIL = "The platform response did not match the supported API shape";
const RESPONSE_HINT = "verify that this terminal client and daemon use the same version";

export const platformRequest = Effect.fn("platform.request")(function* (request: HttpRequestInput) {
  const http = yield* HttpClient;
  const token = yield* resolveAuthToken;
  return yield* http.request<unknown>({...request, token});
});

export function decodePlatform<T>(schema: Schema.Codec<T, unknown>, value: unknown) {
  return Schema.decodeUnknownEffect(schema)(value).pipe(Effect.mapError(() =>
    new UnexpectedError({detail: RESPONSE_DETAIL, suggestion: RESPONSE_HINT})));
}

export function platformInput<T>(schema: Schema.Codec<T, unknown>, value: unknown) {
  return Schema.decodeUnknownEffect(schema, {onExcessProperty: "error"})(value).pipe(
    Effect.mapError(() => invalidInput(INPUT_HINT)));
}

export function invalidInput(detail: string) {
  return new ValidationError({detail, suggestion: INPUT_HINT});
}

export const readPlatformFile = Effect.fn("platform.readFile")(function* (path: string) {
  const stdin = yield* Stdin;
  if (path === "-" && stdin.isTTY) return yield* Effect.fail(invalidInput("Pipe JSON into stdin or name an input file"));
  const file = Bun.file(path);
  if (path !== "-" && file.size > INPUT_MAX_BYTES) return yield* Effect.fail(invalidInput("JSON input must be at most 1 MiB"));
  const text = path === "-" ? yield* stdin.readToEnd : yield* Effect.tryPromise({
    try: () => file.text(), catch: () => invalidInput("The JSON input file could not be read"),
  });
  if (Buffer.byteLength(text) > INPUT_MAX_BYTES) return yield* Effect.fail(invalidInput("JSON input must be at most 1 MiB"));
  return yield* Effect.try({try: (): unknown => JSON.parse(text), catch: () => invalidInput("The input must contain valid JSON")});
});

export const platformResult = Effect.fn("platform.result")(function* (value: unknown) {
  const output = yield* Output;
  yield* output.printJson(value);
});

export function platformPath(base: string, id: string, suffix = "") {
  return `${base}/${encodeURIComponent(id)}${suffix}`;
}

export function platformQuery(values: Record<string, string | number | undefined>) {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(values)) if (value !== undefined) query.set(key, String(value));
  return query.size ? `?${query}` : "";
}
