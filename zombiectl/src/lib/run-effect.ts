// Effect dispatcher — runs an Effect-shaped command, provides the
// MainLayer at the boundary, translates the Exit into a process exit
// code via the shared formatter + EXIT_CODE map.
//
// Owns the cli_command_started / cli_command_finished / cli_error
// analytics triplet so individual command Effects don't have to wire
// it. cli_session_id + cli_device_id are added automatically inside
// the Analytics service from TelemetryRuntime.
//
// Catches via `Effect.catchAllCause` so both typed failures (CliError
// variants) and dies (uncaught exceptions inside the Effect graph)
// route through the formatter — there's no untyped escape.

import { Cause, Effect, Exit, Layer } from "effect";
import { Analytics, AnalyticsLive } from "../services/analytics.ts";
import { Output, OutputStdioLayer, OutputFromStreams } from "../services/output.ts";
import {
  TelemetryRuntime,
  TelemetryRuntimeFromValues,
} from "../services/telemetry-runtime.ts";
import {
  CliConfig,
  CliConfigLive,
  CliConfigFromValues,
} from "../services/config.ts";
import { Credentials, CredentialsLive } from "../services/credentials.ts";
import { HttpClient, HttpClientLive } from "../services/http-client.ts";
import { Browser, BrowserLive } from "../services/browser.ts";
import { Workspaces, WorkspacesLive } from "../services/workspaces.ts";
import { Spinner, SpinnerLive } from "../services/spinner.ts";
import {
  EXIT_CODE,
  UnexpectedError,
  type CliError,
} from "../errors/index.ts";
import {
  EVT_CLI_COMMAND_STARTED,
  EVT_CLI_COMMAND_FINISHED,
  EVT_CLI_ERROR,
} from "../constants/analytics-events.ts";

// Every service MainLayer + the runtime telemetry layer provides.
// Command Effects' R channel must be a subset.
export type MainLayerServices =
  | Analytics
  | Browser
  | CliConfig
  | Credentials
  | HttpClient
  | Output
  | Spinner
  | TelemetryRuntime
  | Workspaces;

// R is the service-set the command Effect needs. The dispatcher provides
// MainLayer; if R is not a subset of what MainLayer covers, the
// `Effect.provide(MainLayer)` call below fails to typecheck — that's
// the compile-time guard that every command's declared service-set is
// actually wired.
export interface RunEffectInput<E extends CliError, R> {
  readonly name: string;
  readonly effect: Effect.Effect<void, E, R>;
  readonly telemetry?: {
    readonly sessionId: string | null;
    readonly deviceId: string | null;
  };
  // Per-invocation overrides commander parsed from argv. Threaded into
  // CliConfig here so command Effects see the same jsonMode/noOpen the
  // pre-Effect dispatcher sees. apiUrl override comes from --api.
  readonly config?: {
    readonly jsonMode?: boolean;
    readonly noOpen?: boolean;
    readonly apiUrl?: string;
    readonly fetchImpl?: import("./http.ts").FetchImpl;
  };
  // Optional stream pair — integration tests inject in-memory streams
  // via runCli's RunCliIo. When set, the dispatcher provisions Output
  // against these instead of process.stdout/stderr so test assertions
  // see the actual command writes.
  readonly streams?: {
    readonly stdout: NodeJS.WritableStream;
    readonly stderr: NodeJS.WritableStream;
  };
}

const formatExit = <E extends CliError>(
  exit: Exit.Exit<void, E>,
): { code: number; rendered: CliError } | null => {
  if (Exit.isSuccess(exit)) return null;
  const failure = Cause.failureOption(exit.cause);
  if (failure._tag === "Some") {
    const err = failure.value;
    return { code: EXIT_CODE[err._tag], rendered: err };
  }
  // Die / interrupt / unknown cause — render as UnexpectedError.
  const detail = Cause.pretty(exit.cause);
  const err = new UnexpectedError({
    detail,
    suggestion: "report this with the output above and the command you ran",
  });
  return { code: EXIT_CODE.UnexpectedError, rendered: err };
};

// Server errors carry a server-side code (UZ-...) and a request_id that
// support workflows grep on. Emit them alongside the detail message so
// the Effect dispatcher produces the same stderr shape as the
// pre-Effect renderApi (`error: <code> <message>\nrequest_id: <id>`).
const renderError = (
  err: CliError,
): Effect.Effect<void, never, Output> =>
  Effect.gen(function* () {
    const output = yield* Output;
    if (err._tag === "ServerError") {
      const tail = err.requestId ? `\nrequest_id: ${err.requestId}` : "";
      yield* output.error(`${err.code} ${err.detail}\n  Suggestion: ${err.suggestion}${tail}`);
      return;
    }
    if (err._tag === "AuthError") {
      const tail = err.requestId ? `\nrequest_id: ${err.requestId}` : "";
      yield* output.error(`${err.code} ${err.detail}\n  Suggestion: ${err.suggestion}${tail}`);
      return;
    }
    yield* output.error(err.message);
  });

const renderAndCount = <E extends CliError>(
  name: string,
  exit: Exit.Exit<void, E>,
): Effect.Effect<number, never, Output | Analytics> =>
  Effect.gen(function* () {
    const analytics = yield* Analytics;
    const formatted = formatExit(exit);
    if (formatted === null) {
      yield* analytics.capture(EVT_CLI_COMMAND_FINISHED, {
        command: name,
        exit_code: "0",
      });
      return 0;
    }
    yield* renderError(formatted.rendered);
    const errorCode =
      formatted.rendered._tag === "ServerError" ||
      formatted.rendered._tag === "AuthError"
        ? formatted.rendered.code
        : formatted.rendered._tag;
    yield* analytics.capture(EVT_CLI_ERROR, {
      command: name,
      error_code: errorCode,
      exit_code: String(formatted.code),
    });
    yield* analytics.capture(EVT_CLI_COMMAND_FINISHED, {
      command: name,
      exit_code: String(formatted.code),
    });
    return formatted.code;
  });

const captureStarted = (name: string): Effect.Effect<void, never, Analytics | CliConfig> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const analytics = yield* Analytics;
    yield* analytics.capture(EVT_CLI_COMMAND_STARTED, {
      command: name,
      json_mode: String(config.jsonMode),
    });
  });

export const runEffect = async <E extends CliError, R extends MainLayerServices>(
  input: RunEffectInput<E, R>,
): Promise<number> => {
  const telemetryLayer = TelemetryRuntimeFromValues({
    sessionId: input.telemetry?.sessionId ?? null,
    deviceId: input.telemetry?.deviceId ?? null,
  });

  const program = Effect.gen(function* () {
    yield* captureStarted(input.name);
    const exit = yield* Effect.exit(input.effect);
    return yield* renderAndCount(input.name, exit);
  });

  // MainLayer is re-composed per call so the per-invocation
  // CliConfigFromValues override actually reaches HttpClient + Analytics
  // (which both consume CliConfig). Pre-baking MainLayer with
  // CliConfigLive would freeze the env-resolved config inside those
  // services; the override here would only land on the leaf CliConfig
  // tag, not the copy threaded into HttpClient. Re-composing keeps the
  // override authoritative.
  const cliConfigLayer =
    input.config !== undefined ? CliConfigFromValues(input.config) : CliConfigLive;
  const httpLayer = HttpClientLive.pipe(Layer.provide(cliConfigLayer));
  const analyticsLayer = AnalyticsLive.pipe(Layer.provide(telemetryLayer));
  const outputLayer =
    input.streams !== undefined
      ? OutputFromStreams(input.streams)
      : OutputStdioLayer;
  const runtime = Layer.mergeAll(
    cliConfigLayer,
    telemetryLayer,
    outputLayer,
    CredentialsLive,
    BrowserLive,
    WorkspacesLive,
    SpinnerLive,
    httpLayer,
    analyticsLayer,
  );
  // The `R extends MainLayerServices` constraint guarantees the residual
  // after the runtime layer is `never`; TypeScript cannot prove the
  // symbolic Exclude<> reduction so a single localised cast at the
  // boundary is the smaller honest seam.
  const provided = program.pipe(Effect.provide(runtime)) as Effect.Effect<
    number,
    never,
    never
  >;
  return await Effect.runPromise(provided);
};
