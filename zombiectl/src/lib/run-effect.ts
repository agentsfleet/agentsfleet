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

import { Cause, Effect, Exit } from "effect";
import { Analytics } from "../services/analytics.ts";
import { Output } from "../services/output.ts";
import { MainLayer } from "../runtime/main-layer.ts";
import { TelemetryRuntime, TelemetryRuntimeFromValues } from "../services/telemetry-runtime.ts";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
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
// Command Effects' R channel must be a subset. Login-only services
// (Browser/Stdin/Crypto) land in commit 2 of this PR.
export type MainLayerServices =
  | Analytics
  | CliConfig
  | Credentials
  | HttpClient
  | Output
  | TelemetryRuntime;

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

const renderAndCount = <E extends CliError>(
  name: string,
  exit: Exit.Exit<void, E>,
): Effect.Effect<number, never, Output | Analytics> =>
  Effect.gen(function* () {
    const output = yield* Output;
    const analytics = yield* Analytics;
    const formatted = formatExit(exit);
    if (formatted === null) {
      yield* analytics.capture(EVT_CLI_COMMAND_FINISHED, {
        command: name,
        exit_code: "0",
      });
      return 0;
    }
    yield* output.error(formatted.rendered.message);
    yield* analytics.capture(EVT_CLI_ERROR, {
      command: name,
      error_code: formatted.rendered._tag,
      exit_code: String(formatted.code),
    });
    yield* analytics.capture(EVT_CLI_COMMAND_FINISHED, {
      command: name,
      exit_code: String(formatted.code),
    });
    return formatted.code;
  });

export const runEffect = async <E extends CliError, R extends MainLayerServices>(
  input: RunEffectInput<E, R>,
): Promise<number> => {
  const telemetryLayer = TelemetryRuntimeFromValues({
    sessionId: input.telemetry?.sessionId ?? null,
    deviceId: input.telemetry?.deviceId ?? null,
  });

  const program = Effect.gen(function* () {
    const analytics = yield* Analytics;
    yield* analytics.capture(EVT_CLI_COMMAND_STARTED, { command: input.name });
    const exit = yield* Effect.exit(input.effect);
    return yield* renderAndCount(input.name, exit);
  });

  // The `R extends MainLayerServices` constraint guarantees the residual
  // after MainLayer + telemetryLayer is `never` at runtime; TypeScript
  // cannot prove the symbolic Exclude<> reduction, so a single localised
  // cast at the boundary is the smaller honest seam than threading
  // higher-kinded type plumbing through every command.
  const provided = program.pipe(
    Effect.provide(MainLayer),
    Effect.provide(telemetryLayer),
  ) as Effect.Effect<number, never, never>;
  return await Effect.runPromise(provided);
};
