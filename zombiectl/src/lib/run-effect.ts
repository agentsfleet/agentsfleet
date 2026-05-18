// Effect dispatcher — runs an Effect-shaped command, provides the
// MainLayer at the boundary, translates the Exit into a process exit
// code via the shared formatter + EXIT_CODE map.
//
// Analytics emit is NOT this layer's responsibility — it lives in
// services/telemetry/command-instrumentation.ts:withCommandInstrumentation,
// applied at the single bind site in program/handlers-bind.ts. The
// dispatcher just runs the Effect and renders the Exit. Mirrors
// Supabase's shared/cli/run.ts handledProgram shape.
//
// Catches via `Effect.exit` so both typed failures (CliError variants)
// and dies (uncaught exceptions inside the Effect graph) route through
// the formatter — there's no untyped escape.

import { Cause, Effect, Exit, Layer, Option } from "effect";
import { Output } from "../services/output.ts";
import {
  mainLayerFor,
  type MainLayerInput,
  type MainLayerServices,
} from "../runtime/main-layer.ts";
import {
  EXIT_CODE,
  UnexpectedError,
  type CliError,
} from "../errors/index.ts";

export type { MainLayerServices } from "../runtime/main-layer.ts";

// R is the service-set the command Effect needs. The dispatcher provides
// MainLayer; if R is not a subset of what MainLayer covers, the
// `Effect.provide(MainLayer)` call below fails to typecheck — that's
// the compile-time guard that every command's declared service-set is
// actually wired.
//
// A — the success value type. `void` is the common case (the dispatcher
// maps success → exit 0). `number` lets a command emit its own exit
// code on success (e.g. `doctor` returns 1 when checks logically fail
// without raising a typed error).
export interface RunEffectInput<A, E extends CliError, R> {
  readonly name: string;
  readonly effect: Effect.Effect<A, E, R>;
  // Pre-built layer mirrors the Supabase pattern (shared/cli/run.ts):
  // compose at one site, provide at the boundary. When omitted, a
  // default layer is built from `mainLayerFor` — used by tests that
  // don't need overrides. Callers with telemetry/config/streams should
  // build the layer once via `mainLayerFor(...)` and pass it here.
  readonly layer?: Layer.Layer<MainLayerServices>;
  // Convenience shortcut for the common case where the caller doesn't
  // build the layer itself. When set, `mainLayerFor(layerInput)` is
  // composed here. `layer` takes precedence if both are provided.
  readonly layerInput?: MainLayerInput;
}

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

const renderAndCount = <A, E extends CliError>(
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

export const runEffect = async <A, E extends CliError, R extends MainLayerServices>(
  input: RunEffectInput<A, E, R>,
): Promise<number> => {
  const program = Effect.gen(function* () {
    const exit = yield* Effect.exit(input.effect);
    return yield* renderAndCount(exit);
  });

  const runtime = input.layer ?? mainLayerFor(input.layerInput);
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
