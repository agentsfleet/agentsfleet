// Shared in-memory layers for the Effect-shaped tenant provider tests.
//
// Extracted when tenant-effect.unit.test.ts passed the repository's 350-line cap.

import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output, OUTPUT_FORMAT, type OutputFormat } from "../src/services/output.ts";
import { outputDouble } from "./helpers-output-double.ts";
import { ServerError, type CliError } from "../src/errors/index.ts";
import { NANOS_PER_USD } from "../src/constants/billing.ts";

export const TENANT_PROVIDER_PATH = "/v1/tenants/me/provider";
export const TENANT_BILLING_PATH = "/v1/tenants/me/billing";
export const ONE_CENT_NANOS = NANOS_PER_USD / 100;

export interface HttpCall {
  readonly path: string;
  readonly method: string;
  readonly body: unknown;
}

export interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly httpCalls: HttpCall[];
}

export const makeRecorder = (): Recorder => ({ stdout: [], stderr: [], httpCalls: [] });

export const outputLayer = (
  rec: Recorder,
  format: OutputFormat = OUTPUT_FORMAT.text,
): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    ...outputDouble(),
    format,
    intro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    info: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    success: (msg, data) =>
      Effect.sync(() =>
        rec.stdout.push(
          format === OUTPUT_FORMAT.json
            ? JSON.stringify(data ?? { message: msg })
            : `ok: ${msg}`,
        ),
      ),
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

export const credentialsLayer = (): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.succeed(Option.some(Redacted.make("test-token"))),
    snapshot: Effect.succeed({ accessToken: Option.none(), savedAt: null, sessionId: null, apiUrl: null, credentialId: null }),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

export const httpClientLayer = (
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

export const configLayer = (
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

export const runWith = <E extends CliError>(
  effect: Effect.Effect<void, E, never>,
): Promise<Exit.Exit<void, E>> => Effect.runPromiseExit(effect);

export const expectFailure = <E extends CliError>(
  exit: Exit.Exit<void, E>,
): E => {
  if (Exit.isSuccess(exit)) throw new Error("expected failure");
  const failure = Option.getOrNull(Cause.findErrorOption(exit.cause));
  if (failure === null) throw new Error("no typed failure in cause");
  return failure;
};
