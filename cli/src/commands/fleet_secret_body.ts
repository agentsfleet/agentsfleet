// Secret-create body resolution — split out of fleet_secret.ts to keep
// that file under the 350-line FLL cap. Two input forms converge here on the
// `data` object the vault POST carries:
//
//   1. the generic `--data <json>` blob (or `--data=@-` stdin), and
//   2. the typed provider flags, in either of two shapes — a named provider
//      (`--provider <id> --api-key <key> --model <m>`) or a custom endpoint
//      (`--provider openai-compatible --base-url <url> --model <m>
//      [--api-key <key>]`) — composing the same
//      `{ provider, model, base_url?, api_key? }` JSON.
//
// `--base-url`'s https check already ran at PARSE time (commander option
// validator, exit 2, no network); the only checks here are the field-pairing
// rules, kept in lockstep with the resolver: `--provider` is required, because
// any ONE typed flag routes here and commander only runs the catalogue parser
// on a flag it sees; `--model` is always required; `--api-key` is required for
// a named provider but OPTIONAL for openai-compatible (a keyless gateway dials
// with no key); openai-compatible ⇔ base_url present.
// Full SSRF validation stays server-side in base_url_guard.zig (typed UZ-* error).

import { Effect } from "effect";
import { ConfigError, ValidationError, type CliError } from "../errors/index.ts";
import {
  OPENAI_COMPATIBLE_PROVIDER,
  SECRET_FIELD_PROVIDER,
  SECRET_FIELD_API_KEY,
  SECRET_FIELD_BASE_URL,
  SECRET_FIELD_MODEL,
} from "../constants/custom-endpoint.ts";

const STDIN_SENTINEL = "@-";
const MISSING_DATA_HINT =
  "missing --data flag. Pipe JSON on stdin with --data=@- or pass --data='{...}'. Stdin form keeps secrets out of shell history.";
const TYPE_STRING = "string" as const;

const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

export interface SecretAddFlags {
  readonly name?: string | undefined;
  readonly data?: string | undefined;
  readonly provider?: string | undefined;
  readonly baseUrl?: string | undefined;
  readonly apiKey?: string | undefined;
  readonly model?: string | undefined;
}

type ParsedData =
  | { readonly ok: true; readonly value: Record<string, unknown> }
  | { readonly ok: false; readonly message: string; readonly suggestion?: string };

// A rejection states the fact; the suggestion is the runnable form that fixes
// it. Kept apart so the renderer's `detail` / `Suggestion:` lines do not both
// carry a usage — printing it twice, once for the shape the caller is not in.
const reject = (message: string, suggestion: string): ParsedData =>
  ({ ok: false, message, suggestion });

// Two usages, because the typed form has two shapes and they reject each
// other's flags. Showing the custom-endpoint line on a named-provider error
// walks the reader into `--base-url is only valid with --provider
// openai-compatible` — advice that produces the next error.
const NAMED_PROVIDER_USAGE =
  `agentsfleet secret create|update <name> --provider <id> --api-key <key> --model <m>`;
const CUSTOM_ENDPOINT_USAGE =
  `agentsfleet secret create|update <name> --provider ${OPENAI_COMPATIBLE_PROVIDER} ` +
  `--base-url https://host/v1 --model <m> [--api-key <key>]`;
// For the caller who has not named a provider yet, so neither shape is wrong.
const PROVIDER_ADD_USAGE = `${NAMED_PROVIDER_USAGE}  (or)  ${CUSTOM_ENDPOINT_USAGE}`;

// The usage that matches the shape the caller is already in.
const usageFor = (isCustom: boolean): string =>
  isCustom ? CUSTOM_ENDPOINT_USAGE : NAMED_PROVIDER_USAGE;

const parseDataObject = (raw: string): ParsedData => {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { ok: false, message: `--data is not valid JSON: ${message}` };
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { ok: false, message: "--data must be a JSON object (not a string, array, or scalar)" };
  }
  const obj = parsed as Record<string, unknown>;
  if (Object.keys(obj).length === 0) {
    return {
      ok: false,
      message: "--data must be a non-empty JSON object — at least one field is required",
    };
  }
  return { ok: true, value: obj };
};

const readStdinJson: Effect.Effect<string, ConfigError> = Effect.tryPromise({
  try: () => Bun.stdin.text(),
  catch: (err) =>
    new ConfigError({
      detail: `failed to read stdin: ${err instanceof Error ? err.message : String(err)}`,
      suggestion: "ensure stdin is not closed and re-pipe the JSON payload",
    }),
});

// Compose the secret JSON from the typed provider flags. Returns the same
// result-bag shape `parseDataObject` uses so the two body sources converge.
const typedProviderBody = (flags: SecretAddFlags): ParsedData => {
  const provider = flags.provider?.trim() ?? "";
  // Trimmed like every sibling flag, and like resolveApiKeyFromEnv on the env
  // slot: an all-whitespace key would otherwise pass the non-empty gate below
  // and store a credential that reports success and can never authenticate.
  const apiKey = flags.apiKey?.trim() ?? "";
  const baseUrl = flags.baseUrl?.trim();
  const model = flags.model?.trim();

  const isCustom = provider === OPENAI_COMPATIBLE_PROVIDER;

  // Any one of the four typed flags engages this path, so it is reachable with
  // no --provider at all — and commander only runs the catalogue parser on a
  // flag it actually sees. Without this rule `--api-key k --model m` composes
  // `provider: ""`, which the server classifies as a provider_key like any
  // other non-sentinel string: stored, reported stored, and never dialable.
  if (provider.length === 0) {
    return reject("the typed form requires --provider", PROVIDER_ADD_USAGE);
  }

  // api_key is required for a named provider; OPTIONAL for an openai-compatible
  // endpoint (a keyless gateway dials with no key) — mirrors the dashboard and
  // the resolver, which only requires a non-empty key for named providers.
  if (!isCustom && apiKey.length === 0) {
    return reject("--provider requires --api-key", NAMED_PROVIDER_USAGE);
  }
  if (isCustom && (baseUrl === undefined || baseUrl.length === 0)) {
    return reject(`provider '${OPENAI_COMPATIBLE_PROVIDER}' requires --base-url`, CUSTOM_ENDPOINT_USAGE);
  }
  if (!isCustom && baseUrl !== undefined && baseUrl.length > 0) {
    return reject(`--base-url is only valid with --provider ${OPENAI_COMPATIBLE_PROVIDER}`, NAMED_PROVIDER_USAGE);
  }
  // model is required to activate ANY self-managed secret — the resolver
  // probe rejects a secret without one, whatever the provider.
  if (model === undefined || model.length === 0) {
    return reject("--provider requires --model", usageFor(isCustom));
  }

  const value: Record<string, unknown> = {
    [SECRET_FIELD_PROVIDER]: provider,
    [SECRET_FIELD_MODEL]: model,
  };
  if (apiKey.length > 0) value[SECRET_FIELD_API_KEY] = apiKey;
  if (baseUrl !== undefined && baseUrl.length > 0) value[SECRET_FIELD_BASE_URL] = baseUrl;
  return { ok: true, value };
};

// Did the caller use the typed form (any of --provider / --base-url /
// --api-key / --model)? `--model` counts so it routes to the typed path and
// hits the clear pairing error (a model with no --provider names no provider
// at all) rather than the generic "missing --data" hint.
// `--data` and the typed flags are mutually exclusive.
const usedTypedForm = (flags: SecretAddFlags): boolean =>
  isString(flags.provider) ||
  isString(flags.baseUrl) ||
  isString(flags.apiKey) ||
  isString(flags.model);

const resolveDataSource = (
  data: string | undefined,
): Effect.Effect<string, CliError> =>
  Effect.gen(function* () {
    if (!isString(data) || data.length === 0) {
      return yield* Effect.fail(
        new ValidationError({
          detail: MISSING_DATA_HINT,
          suggestion: "pass --data='{...}' or --data=@- for stdin",
        }),
      );
    }
    if (data !== STDIN_SENTINEL) return data;
    const raw = yield* readStdinJson;
    if (!raw || raw.trim().length === 0) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "--data=@- but stdin was empty",
          suggestion: "pipe JSON on stdin: cat secret.json | agentsfleet secret create <name> --data=@-",
        }),
      );
    }
    return raw;
  });

// Resolve the secret `data` object from whichever input form the caller
// used: the typed custom-endpoint flags, or the generic `--data` blob.
export const resolveSecretBody = (
  flags: SecretAddFlags,
): Effect.Effect<Record<string, unknown>, CliError> =>
  Effect.gen(function* () {
    if (usedTypedForm(flags)) {
      if (isString(flags.data)) {
        return yield* Effect.fail(
          new ValidationError({
            detail: "pass either --data or the typed --provider/--base-url/--api-key flags, not both",
            suggestion: PROVIDER_ADD_USAGE,
          }),
        );
      }
      const typed = typedProviderBody(flags);
      if (!typed.ok) {
        return yield* Effect.fail(
          new ValidationError({
            detail: typed.message,
            suggestion: typed.suggestion ?? PROVIDER_ADD_USAGE,
          }),
        );
      }
      return typed.value;
    }
    const raw = yield* resolveDataSource(flags.data);
    const validated = parseDataObject(raw);
    if (!validated.ok) {
      return yield* Effect.fail(
        new ValidationError({
          detail: validated.message,
          suggestion: "fix the --data payload and retry",
        }),
      );
    }
    return validated.value;
  });
