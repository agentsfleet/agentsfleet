// commander-bridge — single seam between commander's non-Effect parse
// loop and the new Effect-shaped Analytics + Tracing layers.
//
// Two responsibilities:
//   1. runCommanderParse — wraps program.parseAsync(argv) in an Effect
//      that catches CommanderError. On parse failure (unknown command,
//      missing argument, etc.) emits cli_command_executed with
//      exit_code: 1 through the new Analytics service, so the supabase
//      single-event shape covers commander-only failures too — the
//      withCommandInstrumentation wrapper (which only fires inside the
//      command Effect) doesn't see parse errors.
//   2. mainLayerForCommanderParse — builds the layer with a synthetic
//      CommandRuntime { commandPath: ["__parse__"] } so the analytics
//      emit has a non-empty span name + command label.
//
// Forward-looking — when (c) lands and effect/unstable/cli's
// Command.runWith replaces commander, this entire file is deleted.
// Command.runWith parses + dispatches in a single Effect, so parse
// errors are caught by the outer Effect.exit alongside command
// errors — no bridge needed.

import { Cause, Effect, Exit, Layer, Option } from "effect";
import { CommanderError, type Command } from "commander";
import { commandRuntimeFromValuesLayer } from "../runtime/command-runtime.service.ts";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { analyticsLayer } from "../services/telemetry/analytics.layer.ts";
import { CurrentAnalyticsContext } from "../services/telemetry/analytics-context.ts";
import { tracingLayer } from "../services/telemetry/tracing.layer.ts";
import { telemetryRuntimeLayer } from "../services/telemetry/runtime.layer.ts";
import { EVT_CLI_COMMAND_EXECUTED } from "../services/telemetry/command-instrumentation.ts";

const PARSE_COMMAND_PATH = ["__parse__"] as const;

export interface CommanderParseResult {
  readonly ok: boolean;
  readonly commanderError: CommanderError | undefined;
  readonly otherError: unknown;
}

// runCommanderParse — Effect-wrap the parseAsync call so a parse
// failure can emit cli_command_executed before bubbling out. The
// caller (cli.ts) maps the result to a process exit code.
export function runCommanderParse(
  program: Command,
  argv: ReadonlyArray<string>,
): Effect.Effect<CommanderParseResult> {
  return Effect.gen(function* () {
    const analytics = yield* Analytics;
    const startedAt = Date.now();

    const outcome = yield* Effect.tryPromise({
      try: () => program.parseAsync([...argv], { from: "user" }),
      catch: (err) => err,
    }).pipe(Effect.exit);

    const durationMs = Date.now() - startedAt;

    if (Exit.isFailure(outcome)) {
      const err = unwrapCause(outcome);
      yield* analytics
        .capture(EVT_CLI_COMMAND_EXECUTED, {
          exit_code: 1,
          duration_ms: durationMs,
        })
        .pipe(
          Effect.updateService(CurrentAnalyticsContext, (current) => ({
            ...current,
            command_run_id: "parse",
            command: "__parse__",
          })),
        );

      if (err instanceof CommanderError) {
        return {
          ok: false,
          commanderError: err,
          otherError: undefined,
        } satisfies CommanderParseResult;
      }
      return {
        ok: false,
        commanderError: undefined,
        otherError: err,
      } satisfies CommanderParseResult;
    }

    yield* analytics
      .capture(EVT_CLI_COMMAND_EXECUTED, {
        exit_code: 0,
        duration_ms: durationMs,
      })
      .pipe(
        Effect.updateService(CurrentAnalyticsContext, (current) => ({
          ...current,
          command_run_id: "parse",
          command: "__parse__",
        })),
      );

    return {
      ok: true,
      commanderError: undefined,
      otherError: undefined,
    } satisfies CommanderParseResult;
  }).pipe(Effect.provide(mainLayerForCommanderParse()));
}

function unwrapCause(exit: Exit.Exit<unknown, unknown>): unknown {
  if (Exit.isSuccess(exit)) return undefined;
  const failure = Cause.findErrorOption(exit.cause);
  if (Option.isSome(failure)) return failure.value;
  return Cause.squash(exit.cause);
}

// Layer for the parse-only Effect: CommandRuntime + Analytics +
// telemetry runtime + tracing. NOT the full MainLayer — parse errors
// don't need credentials, http-client, output, etc.
function mainLayerForCommanderParse() {
  return Layer.mergeAll(
    commandRuntimeFromValuesLayer({
      commandPath: [...PARSE_COMMAND_PATH],
      commandRunId: "parse",
    }),
    analyticsLayer.pipe(Layer.provide(telemetryRuntimeLayer)),
    tracingLayer.pipe(Layer.provide(telemetryRuntimeLayer)),
  );
}
