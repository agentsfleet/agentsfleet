// Tenant provider configuration: show / add / delete the active LLM
// posture (platform-managed default vs self-managed key with a named
// secret).
//
// Backed by /v1/tenants/me/provider — the api_key is never returned in
// responses; this CLI only ever displays the resolved metadata (mode,
// provider, model, secret_ref, context_cap_tokens).

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import {
  PROVIDER_MODE,
  formatDollars,
  NANOS_PER_USD,
} from "../constants/billing.ts";
import {
  TENANT_PROVIDER_PATH,
  TENANT_BILLING_PATH,
} from "../lib/api-paths.ts";
import { ValidationError, type CliError } from "../errors/index.ts";

// <$1 left → warn on reset.
const LOW_BALANCE_THRESHOLD_NANOS = NANOS_PER_USD;
const TYPE_NUMBER = "number" as const;
const TYPE_STRING = "string" as const;
const LITERAL = "—" as const;

const isNumber = (value: unknown): value is number => typeof value === TYPE_NUMBER;
const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

interface ProviderResponse {
  readonly mode?: string;
  readonly provider?: string;
  readonly model?: string;
  readonly context_cap_tokens?: number;
  readonly secret_ref?: string | null;
  readonly synthesised_default?: boolean;
  readonly error?: string;
}

interface BillingResponse {
  readonly balance_nanos?: number;
}

interface ProviderAddBody {
  readonly mode: string;
  readonly secret_ref: string;
  readonly model?: string;
}

const renderProviderTable = (
  res: ProviderResponse | null,
): Effect.Effect<void, never, Output> =>
  Effect.gen(function* () {
    const output = yield* Output;
    yield* output.printTable(
      [
        { key: "field", label: "FIELD" },
        { key: "value", label: "VALUE" },
      ],
      [
        { field: "mode", value: res?.mode ?? LITERAL },
        { field: "provider", value: res?.provider ?? LITERAL },
        { field: "model", value: res?.model ?? LITERAL },
        {
          field: "context_cap_tokens",
          value:
            isNumber(res?.context_cap_tokens)
              ? String(res.context_cap_tokens)
              : LITERAL,
        },
        { field: "secret_ref", value: res?.secret_ref ?? LITERAL },
      ],
    );
  });

export const tenantProviderShowEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const output = yield* Output;
  const http = yield* HttpClient;
  const token = yield* resolveAuthToken;

  const res = yield* http.request<ProviderResponse>({
    path: TENANT_PROVIDER_PATH,
    token,
  });

  if (config.jsonMode) {
    yield* output.printJson(res);
    return;
  }

  // The handler surfaces resolver failures via an `error` field — surface
  // it before the table so the operator sees the broken state immediately.
  if (isString(res.error) && res.error.length > 0) {
    const ref = res.secret_ref ?? "(unknown)";
    const msg =
      res.error === "credential_missing"
        ? `⚠ Secret ${ref} is missing from vault — re-add under the same name OR run 'agentsfleet tenant provider delete'.`
        : `⚠ Provider resolver error: ${res.error} (secret_ref=${ref})`;
    yield* output.error(msg);
  }

  yield* renderProviderTable(res);

  if (res.synthesised_default === true) {
    yield* output.info("");
    yield* output.info("(this is the platform default — no tenant_providers row)");
  }
});

export const tenantProviderAddEffectFromArgs = (
  secretRef: string | undefined,
  modelOverride: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;

    if (!secretRef) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "tenant provider create requires --secret <name>",
          suggestion:
            "pick the secret explicitly so the link to your vault entry is clear",
        }),
      );
    }

    const token = yield* resolveAuthToken;
    const body: ProviderAddBody = modelOverride
      ? {
          mode: PROVIDER_MODE.self_managed,
          secret_ref: secretRef,
          model: modelOverride,
        }
      : {
          mode: PROVIDER_MODE.self_managed,
          secret_ref: secretRef,
        };

    const res = yield* http.request<ProviderResponse>({
      path: TENANT_PROVIDER_PATH,
      method: "PUT",
      body,
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }
    yield* output.success(
      `Tenant provider added: mode=${PROVIDER_MODE.self_managed} secret=${secretRef}`,
    );
    yield* output.info("");
    yield* renderProviderTable(res);
    yield* output.info("");
    yield* output.info(
      `Tip: run a test event to verify the key works against ${res.provider ?? secretRef}.`,
    );
  });

// Best-effort low-balance probe — the delete succeeded and is the headline.
// The billing request is swallowed with `Effect.orElseSucceed(() => null)`,
// and the call site runs the whole probe under `Effect.ignore`, so neither a
// flaky billing endpoint nor a token-resolution failure can turn the
// already-printed delete success into a non-zero exit.
const lowBalanceWarning: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output
> = Effect.gen(function* () {
  const output = yield* Output;
  const http = yield* HttpClient;
  const token = yield* resolveAuthToken;
  const billing = yield* http
    .request<BillingResponse>({ path: TENANT_BILLING_PATH, token })
    .pipe(Effect.orElseSucceed(() => null));
  if (billing === null) return;
  const balance =
    isNumber(billing.balance_nanos) ? billing.balance_nanos : null;
  if (balance !== null && balance < LOW_BALANCE_THRESHOLD_NANOS) {
    yield* output.info("");
    yield* output.error(
      `⚠ Tenant balance is low: ${formatDollars(balance)}. Top up via the dashboard before the next event.`,
    );
  }
});

export const tenantProviderDeleteEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const output = yield* Output;
  const http = yield* HttpClient;
  const token = yield* resolveAuthToken;

  const res = yield* http.request<ProviderResponse>({
    path: TENANT_PROVIDER_PATH,
    method: "DELETE",
    token,
  });

  if (config.jsonMode) {
    yield* output.printJson(res);
    return;
  }
  yield* output.success(
    "Custom LLM provider removed — events will now run on agentsfleet's platform default.",
  );
  yield* output.info("");
  yield* renderProviderTable(res);
  // Best-effort: a probe failure must never break the printed delete success.
  yield* lowBalanceWarning.pipe(Effect.ignore);
});
