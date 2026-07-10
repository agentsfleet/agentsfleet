// Secret-add body resolution — split out of fleet_secret.ts to keep
// that file under the 350-line FLL cap. Two input forms converge here on the
// `data` object the vault POST carries:
//
//   1. the generic `--data <json>` blob (or `--data=@-` stdin), and
//   2. the typed custom-endpoint flags (`--provider openai-compatible
//      --base-url <url> --model <m> [--api-key <key>]`) that compose the same
//      `{ provider, model, base_url?, api_key? }` JSON.
//
// `--base-url`'s https check already ran at PARSE time (commander option
// validator, exit 2, no network); the only checks here are the field-pairing
// rules, kept in lockstep with the resolver: `--model` is always required;
// `--api-key` is required for a named provider but OPTIONAL for openai-compatible
// (a keyless gateway dials with no key); openai-compatible ⇔ base_url present.
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

const STDIN_DATA_SENTINEL = "@-";
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
  readonly force?: boolean | undefined;
}

type ParsedData =
  | { readonly ok: true; readonly value: Record<string, unknown> }
  | { readonly ok: false; readonly message: string };

const PROVIDER_ADD_USAGE =
  `agentsfleet secret create <name> --provider ${OPENAI_COMPATIBLE_PROVIDER} ` +
  `--base-url https://host/v1 --model <m> [--api-key <key>]`;

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

// Compose the secret JSON from the typed custom-endpoint flags. Returns the
// same result-bag shape `parseDataObject` uses so the two body sources converge.
const typedProviderBody = (flags: SecretAddFlags): ParsedData => {
  const provider = flags.provider?.trim() ?? "";
  const apiKey = flags.apiKey ?? "";
  const baseUrl = flags.baseUrl?.trim();
  const model = flags.model?.trim();

  const isCustom = provider === OPENAI_COMPATIBLE_PROVIDER;

  // api_key is required for a named provider; OPTIONAL for an openai-compatible
  // endpoint (a keyless gateway dials with no key) — mirrors the dashboard and
  // the resolver, which only requires a non-empty key for named providers.
  if (!isCustom && apiKey.length === 0) {
    return { ok: false, message: `--provider requires --api-key. ${PROVIDER_ADD_USAGE}` };
  }
  if (isCustom && (baseUrl === undefined || baseUrl.length === 0)) {
    return {
      ok: false,
      message: `provider '${OPENAI_COMPATIBLE_PROVIDER}' requires --base-url. ${PROVIDER_ADD_USAGE}`,
    };
  }
  if (!isCustom && baseUrl !== undefined && baseUrl.length > 0) {
    return {
      ok: false,
      message: `--base-url is only valid with --provider ${OPENAI_COMPATIBLE_PROVIDER}`,
    };
  }
  // model is required to activate ANY self-managed secret — the resolver
  // probe rejects a secret without one, whatever the provider.
  if (model === undefined || model.length === 0) {
    return { ok: false, message: `--provider requires --model. ${PROVIDER_ADD_USAGE}` };
  }

  const value: Record<string, unknown> = {
    [SECRET_FIELD_PROVIDER]: provider,
    [SECRET_FIELD_MODEL]: model,
  };
  if (apiKey.length > 0) value[SECRET_FIELD_API_KEY] = apiKey;
  if (baseUrl !== undefined && baseUrl.length > 0) value[SECRET_FIELD_BASE_URL] = baseUrl;
  return { ok: true, value };
};

// Did the caller use the typed custom-endpoint form (any of --provider /
// --base-url / --api-key / --model)? `--model` counts so it routes to the typed
// path and hits the clear pairing error (a model with no --provider/--base-url is
// an incomplete custom endpoint) rather than the generic "missing --data" hint.
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
    if (data !== STDIN_DATA_SENTINEL) return data;
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
          new ValidationError({ detail: typed.message, suggestion: PROVIDER_ADD_USAGE }),
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
