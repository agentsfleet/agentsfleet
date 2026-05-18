// commander-bridge — adapts commander's parse loop to the Effect runtime.
//
// On successful dispatch, the command handler's own
// withCommandInstrumentation wrap (applied in handlers-bind.ts) emits
// the canonical cli_command_executed event. The bridge MUST NOT wrap
// the outer parseAsync with withCommandInstrumentation — doing so
// would emit a second cli_command_executed event per invocation (one
// for the parse stage, one for the real command). Codex review caught
// this duplicate-emit regression.
//
// Parse-stage failures (CommanderError: unknown command, bad flag,
// missing required arg, --help / --version short-circuit) never reach
// any handler, so handlers-bind never fires for them. The bridge emits
// a single cli_command_executed for those cases only, with
// commandPath=["__parse__"] and exit_code=1.
//
// When effect/unstable/cli's Command.runWith replaces commander, parse
// and dispatch run inside one Effect and this bridge can collapse into
// the outer command runner.

import { Cause, Clock, Effect, Exit, Layer, Option } from "effect";
import { CommanderError, type Command } from "commander";
import { commandRuntimeFromValuesLayer } from "../runtime/command-runtime.service.ts";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { analyticsLayer } from "../services/telemetry/analytics.layer.ts";
import { EVT_CLI_COMMAND_EXECUTED } from "../services/telemetry/command-instrumentation.ts";
import { tracingLayer } from "../services/telemetry/tracing.layer.ts";
import { telemetryRuntimeLayer } from "../services/telemetry/runtime.layer.ts";

const PARSE_COMMAND = "__parse__";
const PARSE_COMMAND_RUN_ID = "parse";
const PARSE_COMMAND_PATH = [PARSE_COMMAND] as const;

interface CommanderParseResult {
  readonly ok: boolean;
  readonly commanderError: CommanderError | undefined;
  readonly otherError: unknown;
}

// runCommanderParse — Effect-wrap parseAsync. On success the handler
// has already emitted cli_command_executed via handlers-bind. On
// parse-stage failure, emit exactly one cli_command_executed with the
// __parse__ command label and exit_code=1.
export function runCommanderParse(
  program: Command,
  argv: ReadonlyArray<string>,
): Effect.Effect<CommanderParseResult> {
  return Effect.gen(function* () {
    const startedAt = yield* Clock.currentTimeMillis;
    const outcome = yield* Effect.tryPromise({
      try: () => program.parseAsync([...argv], { from: "user" }),
      catch: (err) => err,
    }).pipe(Effect.exit);

    if (Exit.isFailure(outcome)) {
      const finishedAt = yield* Clock.currentTimeMillis;
      const analytics = yield* Analytics;
      yield* analytics.capture(EVT_CLI_COMMAND_EXECUTED, {
        command: PARSE_COMMAND,
        command_run_id: PARSE_COMMAND_RUN_ID,
        exit_code: 1,
        duration_ms: finishedAt - startedAt,
      });
      const err = unwrapCause(outcome);
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
      commandRunId: PARSE_COMMAND_RUN_ID,
    }),
    analyticsLayer.pipe(Layer.provide(telemetryRuntimeLayer)),
    tracingLayer.pipe(Layer.provide(telemetryRuntimeLayer)),
  );
}
