// Effect-shaped tenant provider handler tests. Mirrors auth-effect /
// workspace-effect / billing-effect: per-test layer composition, no
// process.exit stubs, no module-level mocking.

import { describe, test, expect } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import {
  tenantProviderShowEffect,
  tenantProviderAddEffectFromArgs,
  tenantProviderDeleteEffect,
} from "../src/commands/tenant.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import {
  ServerError,
  ValidationError,
  type CliError,
} from "../src/errors/index.ts";
import {
  PROVIDER_MODE,
  NANOS_PER_USD,
} from "../src/constants/billing.ts";

const TENANT_PROVIDER_PATH = "/v1/tenants/me/provider";
const TENANT_BILLING_PATH = "/v1/tenants/me/billing";
const ONE_CENT_NANOS = NANOS_PER_USD / 100;

interface HttpCall {
  readonly path: string;
  readonly method: string;
  readonly body: unknown;
}

interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly httpCalls: HttpCall[];
}

const makeRecorder = (): Recorder => ({ stdout: [], stderr: [], httpCalls: [] });

const outputLayer = (rec: Recorder): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    info: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    success: (msg) => Effect.sync(() => rec.stdout.push(`ok: ${msg}`)),
    warn: (msg) => Effect.sync(() => rec.stderr.push(`warn: ${msg}`)),
    error: (msg) => Effect.sync(() => rec.stderr.push(`error: ${msg}`)),
    outro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    printJson: (payload) => Effect.sync(() => rec.stdout.push(JSON.stringify(payload))),
    printJsonErr: (payload) => Effect.sync(() => rec.stderr.push(JSON.stringify(payload))),
    printKeyValue: (record) =>
      Effect.sync(() => {
        for (const [k, v] of Object.entries(record)) rec.stdout.push(`  ${k}: ${v}`);
      }),
    printSection: (title) => Effect.sync(() => rec.stdout.push(`# ${title}`)),
    printTable: (_columns, rows) =>
      Effect.sync(() => {
        for (const row of rows) rec.stdout.push(JSON.stringify(row));
      }),
  });

const credentialsLayer = (): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.succeed(Option.some(Redacted.make("test-token"))),
    getSavedAt: Effect.succeed(null),
    getSessionId: Effect.succeed(null),
    getApiUrl: Effect.succeed(null),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

const httpClientLayer = (
  responder: (path: string, method: string) => Effect.Effect<unknown, ServerError>,
  rec: Recorder,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input) => {
      const method = input.method ?? "GET";
      rec.httpCalls.push({ path: input.path, method, body: input.body ?? null });
      return responder(input.path, method) as Effect.Effect<never, ServerError | never>;
    },
  });

const configLayer = (
  overrides: Partial<{ jsonMode: boolean }> = {},
): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode: overrides.jsonMode ?? false,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

const runWith = <E extends CliError>(
  effect: Effect.Effect<void, E, never>,
): Promise<Exit.Exit<void, E>> => Effect.runPromiseExit(effect);

const expectFailure = <E extends CliError>(
  exit: Exit.Exit<void, E>,
): E => {
  if (Exit.isSuccess(exit)) throw new Error("expected failure");
  const failure = Option.getOrNull(Cause.findErrorOption(exit.cause));
  if (failure === null) throw new Error("no typed failure in cause");
  return failure;
};

describe("tenantProviderShowEffect", () => {
  test("GETs provider config and emits table in text mode", async () => {
    const rec = makeRecorder();
    const program = tenantProviderShowEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () =>
            Effect.succeed({
              mode: PROVIDER_MODE.platform,
              provider: "fireworks",
              model: "kimi-k2.6",
              context_cap_tokens: 256000,
              credential_ref: null,
              synthesised_default: true,
            }) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.httpCalls).toEqual([
      { path: TENANT_PROVIDER_PATH, method: "GET", body: null },
    ]);
    expect(rec.stdout.some((line) => line.includes("fireworks"))).toBe(true);
    expect(
      rec.stdout.some((line) => line.includes("platform default")),
    ).toBe(true);
  });

  test("surfaces credential_missing error to stderr while still rendering table", async () => {
    const rec = makeRecorder();
    const program = tenantProviderShowEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () =>
            Effect.succeed({
              mode: PROVIDER_MODE.self_managed,
              provider: "fireworks",
              error: "credential_missing",
              credential_ref: "fw-key",
            }) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stderr.some((line) => /Credential fw-key is missing/.test(line)),
    ).toBe(true);
    expect(
      rec.stdout.some((line) => line.includes("self_managed")),
    ).toBe(true);
  });

  test("--json mode prints raw response and skips warning prose", async () => {
    const rec = makeRecorder();
    const payload = {
      mode: PROVIDER_MODE.self_managed,
      error: "credential_missing",
      credential_ref: "fw-key",
    };
    const program = tenantProviderShowEffect.pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () => Effect.succeed(payload) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout[0]).toBe(JSON.stringify(payload));
    expect(rec.stderr).toEqual([]);
  });
});

describe("tenantProviderAddEffectFromArgs", () => {
  test("PUTs mode=self_managed with credential_ref and prints tip", async () => {
    const rec = makeRecorder();
    const program = tenantProviderAddEffectFromArgs("fw-key", undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () =>
            Effect.succeed({
              mode: PROVIDER_MODE.self_managed,
              provider: "fireworks",
              model: "kimi-k2.6",
              context_cap_tokens: 256000,
              credential_ref: "fw-key",
            }) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.httpCalls).toHaveLength(1);
    const call = rec.httpCalls[0];
    if (!call) throw new Error("expected one http call");
    expect(call.path).toBe(TENANT_PROVIDER_PATH);
    expect(call.method).toBe("PUT");
    expect(call.body).toEqual({
      mode: PROVIDER_MODE.self_managed,
      credential_ref: "fw-key",
    });
    expect(
      rec.stdout.some((line) =>
        /Tip: run a test event to verify the key works against fireworks/.test(line),
      ),
    ).toBe(true);
  });

  test("--model flag forwards as body.model", async () => {
    const rec = makeRecorder();
    const program = tenantProviderAddEffectFromArgs(
      "fw-key",
      "accounts/fireworks/models/kimi-k2.6",
    ).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () =>
            Effect.succeed({
              mode: PROVIDER_MODE.self_managed,
              provider: "fireworks",
              model: "accounts/fireworks/models/kimi-k2.6",
              credential_ref: "fw-key",
            }) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    const call = rec.httpCalls[0];
    if (!call) throw new Error("expected one http call");
    expect(call.body).toEqual({
      mode: PROVIDER_MODE.self_managed,
      credential_ref: "fw-key",
      model: "accounts/fireworks/models/kimi-k2.6",
    });
  });

  test("--json mode prints raw response and skips tip prose", async () => {
    const rec = makeRecorder();
    const payload = {
      mode: PROVIDER_MODE.self_managed,
      provider: "fireworks",
      model: "kimi-k2.6",
      credential_ref: "fw-key",
    };
    const program = tenantProviderAddEffectFromArgs("fw-key", undefined).pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () => Effect.succeed(payload) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout[0]).toBe(JSON.stringify(payload));
    // The success/tip prose is text-mode only; --json short-circuits before it.
    expect(rec.stdout.some((line) => /Tip: run a test event/.test(line))).toBe(
      false,
    );
  });

  test("missing --credential fails ValidationError without making a request", async () => {
    const rec = makeRecorder();
    const program = tenantProviderAddEffectFromArgs(undefined, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () => Effect.succeed({}) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
    expect(rec.httpCalls).toEqual([]);
  });
});

describe("tenantProviderDeleteEffect", () => {
  test("DELETEs and warns on low balance", async () => {
    const rec = makeRecorder();
    const program = tenantProviderDeleteEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === TENANT_PROVIDER_PATH) {
            return Effect.succeed({
              mode: PROVIDER_MODE.platform,
              provider: "fireworks",
              model: "kimi-k2.6",
              context_cap_tokens: 256000,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({
            balance_nanos: 42 * ONE_CENT_NANOS,
          }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.httpCalls.map((c) => `${c.method} ${c.path}`)).toEqual([
      `DELETE ${TENANT_PROVIDER_PATH}`,
      `GET ${TENANT_BILLING_PATH}`,
    ]);
    expect(
      rec.stderr.some((line) =>
        /Tenant balance is low: \$0\.42/.test(line),
      ),
    ).toBe(true);
  });

  test("high balance suppresses warning", async () => {
    const rec = makeRecorder();
    const program = tenantProviderDeleteEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === TENANT_PROVIDER_PATH) {
            return Effect.succeed({
              mode: PROVIDER_MODE.platform,
              provider: "fireworks",
              model: "kimi-k2.6",
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({
            balance_nanos: 999 * ONE_CENT_NANOS,
          }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stderr.some((line) => /Tenant balance is low/.test(line)),
    ).toBe(false);
  });

  test("billing snapshot failure does not break delete success path", async () => {
    const rec = makeRecorder();
    const program = tenantProviderDeleteEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === TENANT_PROVIDER_PATH) {
            return Effect.succeed({
              mode: PROVIDER_MODE.platform,
              provider: "fireworks",
              model: "kimi-k2.6",
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.fail(
            new ServerError({
              detail: "boom",
              suggestion: "retry",
              code: "INTERNAL_ERROR",
              status: 500,
              requestId: null,
            }),
          ) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(
      rec.stdout.some((line) =>
        /Custom LLM provider removed/.test(line),
      ),
    ).toBe(true);
  });

  test("--json mode prints raw response", async () => {
    const rec = makeRecorder();
    const payload = {
      mode: PROVIDER_MODE.platform,
      provider: "fireworks",
      model: "kimi-k2.6",
    };
    const program = tenantProviderDeleteEffect.pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          (path) => {
            if (path === TENANT_PROVIDER_PATH) {
              return Effect.succeed(payload) as Effect.Effect<unknown, ServerError>;
            }
            return Effect.succeed({
              balance_nanos: 999 * ONE_CENT_NANOS,
            }) as Effect.Effect<unknown, ServerError>;
          },
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout[0]).toBe(JSON.stringify(payload));
  });
});
