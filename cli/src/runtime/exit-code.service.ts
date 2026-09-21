// The exit code a command chose for itself.
//
// Most commands answer by succeeding or failing, and the dispatcher maps that
// to 0 or a code from the error table. A few report a VERDICT instead: `doctor`
// succeeds at running every check and then answers 1 because a check failed.
// Nothing went wrong — the news is bad.
//
// Before the cutover that number was simply the Effect's success value, and the
// dispatcher read it. `Command.runWith` discards it: the library's run answers
// `void` whatever the handler returned, so a `doctor` reporting an unreachable
// server would exit 0 and a deployment check in CI would pass while the
// deployment was down.
//
// So the number travels in a service instead of a return value. The entry point
// creates one holder per invocation and reads it once the run is over. Mirrors
// the reference CLI's `ProcessControl.getExitCode`, minus the signal handling
// this CLI has no use for.

import { Context, Effect, Layer } from "effect";

export const SUCCESS_EXIT_CODE = 0;

export interface CommandExitCode {
  readonly set: (code: number) => Effect.Effect<void>;
  readonly get: Effect.Effect<number>;
}

export const CommandExitCode = Context.Service<CommandExitCode>(
  "agentsfleet/runtime/CommandExitCode",
);

/**
 * Records a command's own exit code and discards the value.
 *
 * Wraps a handler that answers a number so it fits the `void` shape the
 * command tree expects, without the number being lost on the way.
 */
export const withManagedExitCode = <E, R>(
  effect: Effect.Effect<number, E, R>,
): Effect.Effect<void, E, R | CommandExitCode> =>
  Effect.gen(function* () {
    const holder = yield* CommandExitCode;
    const code = yield* effect;
    yield* holder.set(code);
  });

/**
 * A fresh holder, and a reader for it.
 *
 * The holder is per-invocation state, so it is created by the entry point
 * rather than being a layer constant — two runs in one process (which is every
 * test file) must not see each other's verdict.
 */
export const makeCommandExitCode = (): {
  readonly layer: Layer.Layer<CommandExitCode>;
  readonly read: () => number;
} => {
  let code = SUCCESS_EXIT_CODE;
  const service: CommandExitCode = {
    set: (next) => Effect.sync(() => { code = next; }),
    get: Effect.sync(() => code),
  };
  return { layer: Layer.succeed(CommandExitCode, service), read: () => code };
};
