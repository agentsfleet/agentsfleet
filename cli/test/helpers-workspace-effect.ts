import { AGE_KEY, ago } from "../src/output/index.ts";
// Shared in-memory layers for the Effect-shaped workspace handler tests.
//
// Extracted when workspace-effect.unit.test.ts passed 1,300 lines and the
// repository's 350-line cap with it. Every workspace suite composes these
// doubles, so a recorder that drifts drifts once.

import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { Analytics } from "../src/services/telemetry/analytics.service.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output, OUTPUT_FORMAT, type OutputFormat } from "../src/services/output.ts";
import { outputDouble } from "./helpers-output-double.ts";
import { Workspaces, type WorkspacesValue } from "../src/services/workspaces.ts";
import { NetworkError, ServerError, type CliError } from "../src/errors/index.ts";

export const WS_ID = "0195b4ba-8d3a-7f13-8abc-000000000010";
export const WS_ID_2 = "0195b4ba-8d3a-7f13-8abc-000000000011";
export const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-000000000001";
export const OTHER_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-000000000002";
export const HTTP_STATUS_CONFLICT = 409;
export const LOCAL_REMOVAL_STEM = "workspace removed from local state";
export const SERVER_DELETION_STEM = "workspace deleted";

export interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly events: Array<{
    event: string;
    properties: Record<string, unknown>;
  }>;
}

export const makeRecorder = (): Recorder => ({ stdout: [], stderr: [], events: [] });

// The register is the double's now, not CliConfig's, so a test that exercises
// the json branch says so here. `success` answers in whichever one it is given,
// the way the real service does.
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
    printJson: (payload) =>
      Effect.sync(() => rec.stdout.push(JSON.stringify(payload))),
    printJsonErr: (payload) =>
      Effect.sync(() => rec.stderr.push(JSON.stringify(payload))),
    printKeyValue: (record) =>
      Effect.sync(() => {
        for (const [k, v] of Object.entries(record))
          rec.stdout.push(`  ${k}: ${v}`);
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

export const workspacesLayer = (state: {
  value: WorkspacesValue;
}): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: Effect.sync(() => state.value),
    save: (next) =>
      Effect.sync(() => {
        state.value = { ...next, items: [...next.items] };
      }),
  });

export interface FakeCredsState {
  token: Option.Option<Redacted.Redacted<string>>;
}

export const credentialsLayer = (state: FakeCredsState): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() => state.token),
    snapshot: Effect.succeed({ accessToken: Option.none(), savedAt: null, sessionId: null, apiUrl: null, credentialId: null }),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

export const httpClientLayer = (
  responder: (
    path: string,
    method: string | undefined,
    input: HttpRequestInput,
  ) => Effect.Effect<unknown, NetworkError | ServerError>,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input) =>
      responder(input.path, input.method, input) as Effect.Effect<
        never,
        ServerError | never
      >,
  });

export const configLayer = (
  overrides: Partial<{
    apiUrl: string;
    dashboardUrl: string;
    accessToken: Option.Option<Redacted.Redacted<string>>;
    jsonMode: boolean;
  }> = {},
): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: overrides.apiUrl ?? "https://api.test.local",
    dashboardUrl: overrides.dashboardUrl ?? "https://dash.test.local",
    accessToken: overrides.accessToken ?? Option.none(),
    jsonMode: overrides.jsonMode ?? false,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

export const runWith = <E extends CliError>(
  effect: Effect.Effect<void, E, never>,
): Promise<Exit.Exit<void, E>> => Effect.runPromiseExit(effect);

export const expectFailure = <E extends CliError>(exit: Exit.Exit<void, E>): E => {
  if (Exit.isSuccess(exit)) throw new Error("expected failure");
  const failure = Option.getOrNull(Cause.findErrorOption(exit.cause));
  if (failure === null) throw new Error("no typed failure in cause");
  return failure;
};
