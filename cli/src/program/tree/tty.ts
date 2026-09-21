// Whether anyone is watching, asked per invocation.
//
// Several commands print a table to a terminal and the JSON envelope to a
// pipe, so they need to know which they have.
//
// The stream to ask is the INVOCATION's, not the process's. `runCli` accepts
// injected stdout, and every test that renders a table passes a stand-in
// carrying `isTTY: true`; reading `process.stdout` here would answer for the
// terminal running the suite instead and silently emit JSON where a table was
// asserted. The Output service already holds the stream this run writes to,
// so the answer comes from there.
//
// The handlers still take the boolean as an argument rather than reading a
// service themselves — a handler that reaches for its own stream cannot be
// unit-tested without a global.

import { Effect } from "effect";
import { Output } from "../../services/output.ts";

export const stdoutIsTty: Effect.Effect<boolean, never, Output> = Effect.gen(
  function* () {
    const output = yield* Output;
    return output.stdoutIsTty;
  },
);
