// The CLI library's writes, routed to the invocation's own streams.
//
// `effect/unstable/cli` renders help documents and parse-error text through
// `Console`, not through this repository's `Output` service. Left alone that
// reaches `globalThis.console` and therefore the real process streams, which
// is wrong twice: a test that injects `io.stdout` sees an empty buffer while
// the text lands on the terminal running the suite, and `runCli`'s promise to
// write only where it was told is quietly broken.
//
// So the entry point provides its own `Console` for the duration of the run.
// `Console.Console` is a Context.Reference with `globalThis.console` as its
// default, so the override is scoped to the Effect and nothing outside it
// changes.
//
// Only `log` and `error` are redirected. Every other method — `table`, `time`,
// `group` — inherits from the real console through the prototype, which is the
// idiom the module's own documentation uses. Redirecting the two the library
// actually calls keeps this a bridge rather than a second console
// implementation to maintain.

import type { Console } from "effect/Console";
import type { WritableStreamLike } from "../../output/capability.ts";
import { writeLine } from "../io.ts";

// `console.log("a", "b")` prints `a b`. The library passes a single
// pre-rendered string today, but matching the real console's join keeps a
// second argument from silently vanishing if that ever changes.
const ARG_SEPARATOR = " " as const;

const format = (args: ReadonlyArray<unknown>): string =>
  args.map((arg) => (typeof arg === "string" ? arg : String(arg))).join(ARG_SEPARATOR);

/**
 * A `Console` writing to the streams this invocation was given.
 *
 * Inherits from the real console so the methods the library never calls stay
 * faithful rather than becoming no-ops that hide a future caller.
 */
export const consoleForStreams = (
  stdout: WritableStreamLike,
  stderr: WritableStreamLike,
): Console =>
  Object.assign(Object.create(globalThis.console) as Console, {
    log: (...args: ReadonlyArray<unknown>): void => {
      writeLine(stdout, format(args));
    },
    error: (...args: ReadonlyArray<unknown>): void => {
      writeLine(stderr, format(args));
    },
  });
