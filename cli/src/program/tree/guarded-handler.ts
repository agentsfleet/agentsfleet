// Every handler, gated on the same question.
//
// `Command.provideEffectDiscard` runs an effect before a handler — but before
// the handler of the ONE command it is attached to. The root has no handler of
// its own, so attaching there gates nothing; a signed-out `agentsfleet list`
// would sail past and fail later, deeper, with a worse message.
//
// So the gate is composed into each handler instead, through this one helper.
// Writing `guardedHandler` where a command would have written
// `Command.withHandler` is the whole difference, and a command that forgets it
// is a command with no auth check — which is why every leaf in the tree goes
// through here rather than each file remembering.

import { Effect } from "effect";
import { Command } from "effect/unstable/cli";
import {
  guardGate,
  type CommandGuard,
  type GuardRefused,
} from "../../runtime/guard.service.ts";

/**
 * `Command.withHandler`, with the auth and deployment refusal in front of it.
 *
 * The gate runs after the parser — it is inside the handler, and a handler is
 * only reached once the invocation parsed — so a mistyped flag reports the
 * flag rather than demanding a login for a command that was never going to
 * run.
 */
export const guardedHandler =
  <Input, A, E, R>(handler: (input: Input) => Effect.Effect<A, E, R>) =>
  <const Name extends string, E0, R0, ContextInput>(
    self: Command.Command<Name, Input, ContextInput, E0, R0>,
  ): Command.Command<
    Name,
    Input,
    ContextInput,
    E0 | E | GuardRefused,
    R0 | R | CommandGuard
  > =>
    Command.withHandler((input: Input) =>
      Effect.andThen(guardGate, () => handler(input)),
    )(self);
