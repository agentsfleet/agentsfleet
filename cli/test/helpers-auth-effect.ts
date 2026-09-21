// Shared fixtures and doubles for the auth status and logout suites.
//
// Extracted when auth-effect.unit.test.ts passed the repository's 350-line cap;
// the suites that read them are unchanged.

import { AGE_KEY, ago } from "../src/output/index.ts";
import { Effect, Exit, Layer, Option, Redacted } from "effect";
import { Analytics } from "../src/services/telemetry/analytics.service.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output, OUTPUT_FORMAT, type OutputFormat } from "../src/services/output.ts";
import { outputDouble } from "./helpers-output-double.ts";
import { ServerError, type CliError } from "../src/errors/index.ts";

export interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly events: Array<{ event: string; properties: Record<string, unknown> }>;
  readonly credentialOps: string[];
}

export const makeRecorder = (): Recorder => ({
  stdout: [],
  stderr: [],
  events: [],
  credentialOps: [],
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
        for (const row of rows) rec.stdout.push(JSON.stringify(row));
      }),
    // Mirrors what entityTable renders, so a test reads the age a user sees.
    printEntityTable: (_spec, rows) =>
      Effect.sync(() => {
        for (const row of rows)
          rec.stdout.push(JSON.stringify({ ...row, [AGE_KEY]: ago(row[AGE_KEY]) }));
      }),
  });

export const analyticsLayer = (rec: Recorder): Layer.Layer<Analytics> =>
  Layer.succeed(Analytics, {
    capture: (event, properties = {}) =>
      Effect.sync(() => {
        rec.events.push({ event, properties });
      }),
    identify: () => Effect.void,
    alias: () => Effect.void,
    groupIdentify: () => Effect.void,
  });

export interface FakeCredsState {
  token: Option.Option<Redacted.Redacted<string>>;
  savedAt: number | null;
  sessionId: string | null;
  apiUrl: string | null;
}

export const credentialsLayer = (
  state: FakeCredsState,
  rec: Recorder,
): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() => state.token),
    snapshot: Effect.sync(() => ({
      accessToken: state.token,
      savedAt: state.savedAt,
      sessionId: state.sessionId,
      apiUrl: null,
      credentialId: null,
    })),
    saveAccessToken: (input) =>
      Effect.sync(() => {
        state.token = Option.some(input.token);
        state.savedAt = Date.now();
        state.sessionId = input.sessionId;
        state.apiUrl = input.apiUrl ?? null;
        rec.credentialOps.push("save");
      }),
    clearAccessToken: Effect.sync(() => {
      state.token = Option.none();
      state.savedAt = null;
      state.sessionId = null;
      rec.credentialOps.push("clear");
    }),
  });

export const httpClientLayer = (
  responder: (path: string) => Effect.Effect<unknown, ServerError>,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input) => responder(input.path) as Effect.Effect<never, ServerError | never>,
  });

export const configLayer = (overrides: Partial<{
  apiUrl: string;
  dashboardUrl: string;
  accessToken: Option.Option<Redacted.Redacted<string>>;
  jsonMode: boolean;
}> = {}): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: overrides.apiUrl ?? "https://api.test.local",
    dashboardUrl: overrides.dashboardUrl ?? "https://dash.test.local",
    accessToken: overrides.accessToken ?? Option.none(),
    jsonMode: overrides.jsonMode ?? false,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

export const unused = <T>(): Layer.Layer<T> =>
  Layer.empty as unknown as Layer.Layer<T>;

export const runWith = <E extends CliError>(
  effect: Effect.Effect<void, E, never>,
): Promise<Exit.Exit<void, E>> => Effect.runPromiseExit(effect);
