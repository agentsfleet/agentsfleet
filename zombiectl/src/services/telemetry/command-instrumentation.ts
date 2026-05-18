// withCommandInstrumentation — wraps a command Effect with tracing
// span + analytics emit. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/command-instrumentation.ts.
//
// Single event emitted: `cli_command_executed` with
//   { exit_code: 0 | 1, duration_ms }
// + auto-merged CurrentAnalyticsContext (command_run_id, command,
// flags_used, flag_values from the wrapper). The span (Effect.withSpan)
// carries the error tag via the NDJSON exporter's status.exit._tag
// extraction — no separate `cli_error` event.
//
// `analytics: false` disables the analytics emit but keeps tracing —
// used by commands that emit their own analytics with finer-grained
// properties (e.g. the auth flow).

import { Clock, Effect, Exit } from "effect";
import {
  CommandRuntime,
  getCommandRuntimeCommand,
  getCommandRuntimeSpanName,
} from "../../runtime/command-runtime.service.ts";
import { withAnalyticsContext } from "./analytics-context.ts";
import { Analytics } from "./analytics.service.ts";

export const EVT_CLI_COMMAND_EXECUTED = "cli_command_executed";

export interface CommandInstrumentationOptions<
  Flags extends Record<string, unknown> = Record<string, never>,
> {
  readonly analytics?: boolean;
  readonly flags?: Flags;
  readonly allowedFlagValues?: ReadonlyArray<Extract<keyof Flags, string>>;
}

function toCliFlagName(key: string): string {
  return key.replace(/[A-Z]/g, (char) => `-${char.toLowerCase()}`);
}

function extractFlagsUsed(args: ReadonlyArray<string>): ReadonlyArray<string> {
  const used = new Set<string>();
  for (const arg of args) {
    if (arg === undefined || !arg.startsWith("--")) continue;
    const raw = arg.slice(2);
    const [flagName] = raw.split("=", 2);
    if (flagName === undefined || flagName.length === 0) continue;
    used.add(flagName);
  }
  return [...used].sort((a, b) => a.localeCompare(b));
}

function extractAllowedFlagValues<Flags extends Record<string, unknown>>(
  flags: Flags,
  allowedFlagValues: ReadonlyArray<Extract<keyof Flags, string>>,
  flagsUsed: ReadonlyArray<string>,
): Record<string, unknown> {
  const usedFlagSet = new Set(flagsUsed);
  const entries: Array<readonly [string, unknown]> = [];
  for (const key of allowedFlagValues) {
    const flagName = toCliFlagName(key);
    if (!usedFlagSet.has(flagName)) continue;
    const value = flags[key];
    if (value === undefined) continue;
    entries.push([flagName, value]);
  }
  return Object.fromEntries(entries);
}

function hasFlags<Flags extends Record<string, unknown>>(
  options: CommandInstrumentationOptions<Flags> | undefined,
): options is CommandInstrumentationOptions<Flags> & { readonly flags: Flags } {
  return options?.flags !== undefined;
}

function withCommandTracingImplementation() {
  return <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.gen(function* () {
      const commandRuntime = yield* CommandRuntime;
      const command = getCommandRuntimeCommand(commandRuntime);
      return yield* Effect.gen(function* () {
        yield* Effect.annotateCurrentSpan({
          command_run_id: commandRuntime.commandRunId,
          command,
        });
        return yield* self;
      }).pipe(Effect.withSpan(getCommandRuntimeSpanName(commandRuntime)));
    });
}

function withCommandAnalyticsImplementation<Flags extends Record<string, unknown>>(
  options: CommandInstrumentationOptions<Flags> | undefined,
  argv: ReadonlyArray<string>,
) {
  return <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.gen(function* () {
      const commandRuntime = yield* CommandRuntime;
      const command = getCommandRuntimeCommand(commandRuntime);
      return yield* Effect.gen(function* () {
        yield* Effect.annotateCurrentSpan({
          command_run_id: commandRuntime.commandRunId,
          command,
        });
        const analytics = yield* Analytics;
        const startedAt = yield* Clock.currentTimeMillis;
        const flagsUsed = extractFlagsUsed(argv);
        const flagValues = hasFlags(options)
          ? extractAllowedFlagValues(
              options.flags,
              options.allowedFlagValues ?? [],
              flagsUsed,
            )
          : {};
        const analyticsContext = {
          command_run_id: commandRuntime.commandRunId,
          command,
          flags_used: flagsUsed,
          flag_values: flagValues,
        } as const;

        const exit = yield* self.pipe(withAnalyticsContext(analyticsContext), Effect.exit);
        const finishedAt = yield* Clock.currentTimeMillis;

        yield* analytics
          .capture(EVT_CLI_COMMAND_EXECUTED, {
            exit_code: Exit.isSuccess(exit) ? 0 : 1,
            duration_ms: finishedAt - startedAt,
          })
          .pipe(withAnalyticsContext(analyticsContext));

        if (Exit.isFailure(exit)) {
          return yield* Effect.failCause(exit.cause);
        }
        return exit.value;
      }).pipe(Effect.withSpan(getCommandRuntimeSpanName(commandRuntime)));
    });
}

// Two overloads + one implementation — matches Supabase signature
// exactly so the (c) migration is a no-op for every call site.
export function withCommandInstrumentation(): <A, E, R>(
  self: Effect.Effect<A, E, R>,
) => Effect.Effect<A, E, R | Analytics | CommandRuntime>;
export function withCommandInstrumentation<Flags extends Record<string, unknown>>(
  options: CommandInstrumentationOptions<Flags>,
): <A, E, R>(
  self: Effect.Effect<A, E, R>,
) => Effect.Effect<A, E, R | Analytics | CommandRuntime>;
export function withCommandInstrumentation<Flags extends Record<string, unknown>>(
  options?: CommandInstrumentationOptions<Flags>,
) {
  if (options?.analytics === false) {
    return withCommandTracingImplementation();
  }
  // Supabase reads argv from Stdio service; usezombie reads
  // process.argv.slice(2) directly. When (c) lands and effect/unstable/
  // cli's Command.runWith is in place, argv is provided by the Command
  // primitive — the read site here moves from process.argv to the
  // Command-provided value but the consumer code is identical.
  return withCommandAnalyticsImplementation(options, process.argv.slice(2));
}
