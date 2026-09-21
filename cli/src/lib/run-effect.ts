// Effect dispatcher — runs an Effect-shaped command, provides the
// MainLayer at the boundary, translates the Exit into a process exit
// code via the shared formatter + EXIT_CODE map.
//
// Analytics emit is NOT this layer's responsibility — it lives in
// services/telemetry/command-instrumentation.ts:withCommandInstrumentation,
// applied once around the whole run in cli.ts. The
// dispatcher just runs the Effect and renders the Exit. Mirrors
// Supabase's shared/cli/run.ts handledProgram shape.
//
// Catches via `Effect.exit` so both typed failures (CliError variants)
// and dies (uncaught exceptions inside the Effect graph) route through
// the formatter — there's no untyped escape.

import { Cause, Effect, Exit, Option } from "effect";
import { Output } from "../services/output.ts";
import {
  type MainLayerServices,
} from "../runtime/main-layer.ts";
import {
  EXIT_CODE,
  UnexpectedError,
  type CliError,
} from "../errors/index.ts";

export type { MainLayerServices };

const FALLBACK_EXIT_CODE = 1;

const formatExit = <A, E extends CliError>(
  exit: Exit.Exit<A, E>,
): { code: number; rendered: CliError } | null => {
  if (Exit.isSuccess(exit)) {
    // Numeric success value = command-managed exit code (e.g. doctor's
    // ok ? 0 : 1). Non-numeric success = exit 0. The dispatcher swallows
    // the "rendered" hint for success cases.
    return typeof exit.value === "number" && exit.value !== 0
      ? { code: exit.value, rendered: { _tag: "UnexpectedError" } as CliError }
      : null;
  }
  const failure = Cause.findErrorOption(exit.cause);
  if (Option.isSome(failure)) {
    const err = failure.value;
    return { code: EXIT_CODE[err._tag] ?? FALLBACK_EXIT_CODE, rendered: err };
  }
  // Die / interrupt / unknown cause — render as UnexpectedError.
  const detail = Cause.pretty(exit.cause);
  const err = new UnexpectedError({
    detail,
    suggestion: "report this with the output above and the command you ran",
  });
  return { code: EXIT_CODE.UnexpectedError ?? FALLBACK_EXIT_CODE, rendered: err };
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

/**
 * Renders a finished Exit and answers its process exit code.
 *
 * Exported because the entry point shares it. The CLI tree's own failures are
 * rendered by the library, but a command that fails with one of THIS
 * repository's errors still has to produce the `UZ-*` code, the suggestion and
 * the request id that support workflows grep for — and a second renderer at
 * the entry point would drift from this one the first time a variant changed.
 */
export const renderAndCount = <A, E extends CliError>(
  exit: Exit.Exit<A, E>,
): Effect.Effect<number, never, Output> =>
  Effect.gen(function* () {
    const formatted = formatExit(exit);
    if (formatted === null) return 0;
    // Numeric success exit codes (doctor's ok ? 0 : 1) skip the error
    // render — the command already wrote its own report.
    if (Exit.isSuccess(exit)) return formatted.code;
    yield* renderError(formatted.rendered);
    return formatted.code;
  });
