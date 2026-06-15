// Shared Effect stub layers + runner for the memory read-verb unit tests
// (the helpers-cli-state.ts pattern). Stubs sit at system boundaries only:
// HttpClient (network), Workspaces/Credentials (disk), Output (streams).

import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";

import { CliConfig } from "../src/services/config.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { Credentials } from "../src/services/credentials.ts";
import type { CliError, NetworkError, ServerError } from "../src/errors/index.ts";

export const MEMORY_TEST_WS_ID = "01900000-0000-7000-8000-0000005e4e71";

export interface CapturedOutput {
  infos: string[];
  jsons: unknown[];
  tables: Array<{ columns: ReadonlyArray<{ key: string; label: string }>; rows: ReadonlyArray<Record<string, unknown>> }>;
}

export const newCapture = (): CapturedOutput => ({ infos: [], jsons: [], tables: [] });

export const configLayer = (jsonMode: boolean): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.some(Redacted.make("tok_unit")),
    jsonMode,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

export const outputLayer = (cap: CapturedOutput): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: () => Effect.void,
    info: (msg) => Effect.sync(() => { cap.infos.push(msg); }),
    success: () => Effect.void,
    warn: () => Effect.void,
    error: () => Effect.void,
    outro: () => Effect.void,
    printJson: (payload) => Effect.sync(() => { cap.jsons.push(payload); }),
    printJsonErr: () => Effect.void,
    printKeyValue: () => Effect.void,
    printSection: () => Effect.void,
    printTable: (columns, rows) => Effect.sync(() => { cap.tables.push({ columns, rows }); }),
  });

export const httpLayerReturning = (envelope: unknown, paths: string[]): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: <T,>(input: HttpRequestInput): Effect.Effect<T, NetworkError | ServerError> => {
      paths.push(input.path);
      return Effect.succeed(envelope as T);
    },
  });

export const httpLayerFailing = (err: NetworkError | ServerError): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: () => Effect.fail(err),
  });

export const workspacesLayer = (): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: Effect.succeed({ current_workspace_id: MEMORY_TEST_WS_ID, items: [] }),
    save: () => Effect.die("save should not be called"),
  });

export const credentialsLayer = (): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.succeed(Option.none()),
    getSavedAt: Effect.die("should not be called"),
    getSessionId: Effect.die("should not be called"),
    getApiUrl: Effect.die("should not be called"),
    saveAccessToken: () => Effect.die("should not be called"),
    clearAccessToken: Effect.die("should not be called"),
  });

export interface RunOptions {
  jsonMode?: boolean;
  http: Layer.Layer<HttpClient>;
  cap: CapturedOutput;
  workspaces?: Layer.Layer<Workspaces>;
}

export const runWith = <E extends CliError>(
  effect: Effect.Effect<void, E, CliConfig | Output | HttpClient | Workspaces | Credentials>,
  opts: RunOptions,
): Promise<Exit.Exit<void, E>> =>
  Effect.runPromiseExit(
    effect.pipe(
      Effect.provide(configLayer(opts.jsonMode ?? false)),
      Effect.provide(outputLayer(opts.cap)),
      Effect.provide(opts.http),
      Effect.provide(opts.workspaces ?? workspacesLayer()),
      Effect.provide(credentialsLayer()),
    ),
  );

export const failureOf = <E extends CliError>(exit: Exit.Exit<void, E>): E | null => {
  if (!Exit.isFailure(exit)) return null;
  return Option.getOrNull(Cause.findErrorOption(exit.cause));
};
