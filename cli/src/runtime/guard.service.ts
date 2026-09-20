// The auth guard, as something the command tree can run.
//
// The question this answers — may this command run with the credential and
// target the invocation resolved? — has to be asked AFTER the parser and
// BEFORE the handler. Asking it any earlier inverts the two failures a
// person can make at once: someone who
// mistypes a flag AND is signed out should be told about the flag, because
// that is the one they can fix without leaving the terminal. Being sent
// through a login only to come back to the same typo is the worst outcome of
// the two orderings.
//
// `Command.provideEffectDiscard` on the root is the tree's own version of that
// hook, and it runs for every subcommand. The refusal itself is still decided
// by the pure `guardCommand`; this only carries the verdict to the place the
// tree can act on it.

import { Context, Effect } from "effect";

export interface CommandGuard {
  /** Refuses the invocation, or passes. Written and exited by the entry. */
  readonly check: Effect.Effect<void, GuardRefused>;
}

export const CommandGuard = Context.Service<CommandGuard>(
  "agentsfleet/runtime/CommandGuard",
);

/**
 * A refusal that has already been written.
 *
 * It carries no message because the entry rendered it at the point of
 * decision, where it knew the register and the streams. Re-rendering here
 * would print the refusal twice.
 */
export class GuardRefused {
  readonly _tag = "GuardRefused" as const;
  constructor(readonly exitCode: number) {}
}

/** The gate the root command runs before any handler. */
export const guardGate: Effect.Effect<void, GuardRefused, CommandGuard> =
  Effect.flatMap(CommandGuard, (guard) => guard.check);
