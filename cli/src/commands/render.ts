// The `--json` fork, in one place.
//
// Forty-seven handler sites wrote this branch by hand:
//
//   if (config.jsonMode) { yield* output.printJson(payload); return; }
//   ...then built a table from the same values, separately
//
// Two renderings derived independently from one response is how they drift: a
// field added to the table and not to the payload is invisible until a script
// that parses `--json` goes looking for it. Here the machine payload is the
// argument and the table is a function OF that payload, so the table cannot
// show a value `--json` does not carry.

import { Effect } from "effect";

import type { CommandContext } from "./context.ts";

/** How one command answers, in both registers. */
export interface Rendering<A> {
  /** What `--json` prints. The table is derived from this. */
  readonly json: A;
  /** What a terminal prints, given the same payload. */
  readonly human: (payload: A) => Effect.Effect<void>;
}

/**
 * Print one result in whichever register the caller asked for.
 *
 * Takes the context rather than `config` and `output` separately, so a handler
 * that has already resolved its context passes one value and cannot pair the
 * `jsonMode` of one invocation with the `output` of another.
 */
export const render = <A>(
  ctx: CommandContext,
  rendering: Rendering<A>,
): Effect.Effect<void> =>
  ctx.config.jsonMode
    ? ctx.output.printJson(rendering.json)
    : rendering.human(rendering.json);
