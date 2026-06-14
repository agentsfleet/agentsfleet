// Tenant provider configuration: show / add / delete the active LLM
// posture (platform-managed default vs self-managed key with a named
// credential).
//
// Backed by /v1/tenants/me/provider — the api_key is never returned in
// responses; this CLI only ever displays the resolved metadata (mode,
// provider, model, credential_ref, context_cap_tokens).

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
  readonly credential_ref?: string | null;
  readonly synthesised_default?: boolean;
  readonly error?: string;
}

interface BillingResponse {
  readonly balance_nanos?: number;
}

interface ProviderAddBody {
  readonly mode: string;
  readonly credential_ref: string;
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
        { field: "credential_ref", value: res?.credential_ref ?? LITERAL },
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
    const ref = res.credential_ref ?? "(unknown)";
    const msg =
      res.error === "credential_missing"
        ? `⚠ Credential ${ref} is missing from vault — re-add under the same name OR run 'agentsfleet tenant provider delete'.`
        : `⚠ Provider resolver error: ${res.error} (credential_ref=${ref})`;
    yield* output.error(msg);
  }

  yield* renderProviderTable(res);

  if (res.synthesised_default === true) {
    yield* output.info("");
    yield* output.info("(this is the platform default — no tenant_providers row)");
  }
});

export const tenantProviderAddEffectFromArgs = (
  credentialRef: string | undefined,
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

    if (!credentialRef) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "tenant provider add requires --credential <name>",
          suggestion:
            "pick the credential explicitly so the link to your vault entry is clear",
        }),
      );
    }

    const token = yield* resolveAuthToken;
    const body: ProviderAddBody = modelOverride
      ? {
          mode: PROVIDER_MODE.self_managed,
          credential_ref: credentialRef,
          model: modelOverride,
        }
      : {
          mode: PROVIDER_MODE.self_managed,
          credential_ref: credentialRef,
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
      `Tenant provider added: mode=${PROVIDER_MODE.self_managed} credential=${credentialRef}`,
    );
    yield* output.info("");
    yield* renderProviderTable(res);
    yield* output.info("");
    yield* output.info(
      `Tip: run a test event to verify the key works against ${res.provider ?? credentialRef}.`,
    );
  });

// Best-effort low-balance probe — the reset succeeded and is the headline.
// `Effect.orElseSucceed(() => null)` swallows transport errors so a flaky
// billing endpoint never breaks the delete success path.
const lowBalanceWarning: Effect.Effect<
  void,
  never,
  CliConfig | Credentials | HttpClient | Output
> = Effect.gen(function* () {
  const output = yield* Output;
  const http = yield* HttpClient;
  const token = yield* resolveAuthToken.pipe(Effect.orElseSucceed(() => undefined));
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
    "Custom LLM provider removed — events will now run on usezombie's platform default.",
  );
  yield* output.info("");
  yield* renderProviderTable(res);
  yield* lowBalanceWarning;
});
