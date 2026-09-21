import { AGE_KEY, ago } from "../src/output/index.ts";
// Shared in-memory layers for the Effect-shaped billing handler tests.
//
// Extracted when billing-effect.unit.test.ts passed the repository's 350-line
// cap. Both billing suites compose these doubles.

import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output, OUTPUT_FORMAT, type OutputFormat } from "../src/services/output.ts";
import { outputDouble } from "./helpers-output-double.ts";
import { ServerError, type CliError } from "../src/errors/index.ts";
import { CHARGE_TYPE, NANOS_PER_USD, PROVIDER_MODE } from "../src/constants/billing.ts";

export const BILLING_PATH = "/v1/tenants/me/billing";
export const CHARGES_PATH_PREFIX = "/v1/tenants/me/billing/charges";
export const ONE_CENT_NANOS = NANOS_PER_USD / 100;
export const TEST_RECORDED_AT_MS = 1_000_000 as const;
export const TEST_CHARGE_NANOS = 1000 as const;

export interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly httpCalls: string[];
  // The rendered rows, not just their count. A grouping assertion that reads
  // only `TABLE:n` passes on a table with the right number of rows and the
  // wrong money in them, which is the defect the grouping key exists to stop.
  readonly tables: ReadonlyArray<Record<string, unknown>>[];
}

export const makeRecorder = (): Recorder => ({
  stdout: [],
  stderr: [],
  httpCalls: [],
  tables: [],
});

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
        rec.stdout.push(`TABLE:${rows.length}`);
        rec.tables.push(rows as ReadonlyArray<Record<string, unknown>>);
      }),
    printEntityTable: (_spec, rows) =>
      Effect.sync(() => {
        rec.stdout.push(`TABLE:${rows.length}`);
        const aged = rows.map((row) => ({ ...row, [AGE_KEY]: ago(row[AGE_KEY]) }));
        rec.tables.push(aged as ReadonlyArray<Record<string, unknown>>);
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
  responder: (path: string) => Effect.Effect<unknown, ServerError>,
  rec: Recorder,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input) => {
      rec.httpCalls.push(input.path);
      return responder(input.path) as Effect.Effect<never, ServerError | never>;
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

export const RECEIVE_ROW = {
  event_id: "evt_1",
  charge_type: CHARGE_TYPE.receive,
  posture: PROVIDER_MODE.platform,
  model: "kimi-k2.6",
  credit_deducted_nanos: ONE_CENT_NANOS,
  token_count_input: null,
  token_count_output: null,
  recorded_at: TEST_RECORDED_AT_MS,
};
export const STAGE_ROW = {
  event_id: "evt_1",
  charge_type: CHARGE_TYPE.stage,
  posture: PROVIDER_MODE.platform,
  model: "kimi-k2.6",
  credit_deducted_nanos: 2 * ONE_CENT_NANOS,
  token_count_input: 820,
  token_count_output: 1040,
  recorded_at: 1_000_005,
};
