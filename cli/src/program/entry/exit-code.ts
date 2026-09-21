// One exit code from two error families.
//
// After the cutover a failed run carries one of two kinds of error, and they
// answer the question differently:
//
//   - this repository's own `CliError` union — AuthError, NetworkError and the
//     rest — whose codes come from the `EXIT_CODE` table so a script can tell
//     a network failure (2) from a validation one (4).
//   - the CLI library's `CliError.CliError` — every parse and usage failure,
//     plus `ShowHelp`.
//
// The library's own codes are NOT used. It answers 1 for any usage failure,
// while this CLI has always answered 4, and a script branching on 4 to tell
// "you typed it wrong" from "the server said no" would silently start seeing
// the same number for both. So the whole family maps to ValidationError.
//
// `ShowHelp` is the exception inside that family, and it is the reason this
// cannot be a blanket rule: `agentsfleet workspace` with no verb, and any
// explicit `--help`, both FAIL with `ShowHelp` carrying an empty `errors`
// array. That is a help document rendered on request, so it exits 0. The same
// error with a non-empty `errors` array is a real usage failure and exits 4.

import { Cause, Runtime } from "effect";
import { CliError } from "effect/unstable/cli";
import { EXIT_CODE } from "../../errors/index.ts";
import { GuardRefused } from "../../runtime/guard.service.ts";

const INTERRUPT_EXIT_CODE = 130;
const SUCCESS_EXIT_CODE = 0;

interface MaybeTagged {
  readonly _tag?: string;
}

const domainExitCode = (error: unknown): number | undefined => {
  if (typeof error !== "object" || error === null) return undefined;
  const tag = (error as MaybeTagged)._tag;
  if (tag === undefined) return undefined;
  return Object.hasOwn(EXIT_CODE, tag) ? EXIT_CODE[tag as keyof typeof EXIT_CODE] : undefined;
};

// A help document the user asked for is not a failure. The library marks it
// by exit code rather than by type, so the marker is what gets read — an
// empty `errors` array is help on request, a populated one is a usage error
// shown WITH help.
const isRequestedHelp = (error: unknown): boolean =>
  error instanceof CliError.ShowHelp &&
  Runtime.getErrorExitCode(error) === SUCCESS_EXIT_CODE;

/**
 * The process exit code for a failed run.
 *
 * An interrupt answers 130 before anything else is asked: a cancelled run has
 * no error to classify, because the fibre was killed rather than failed.
 */
export const exitCodeForFailure = (cause: Cause.Cause<unknown>): number => {
  if (Cause.hasInterruptsOnly(cause)) return INTERRUPT_EXIT_CODE;
  const error: unknown = Cause.squash(cause);
  // A refusal the guard already wrote names its own code and needs no render.
  if (error instanceof GuardRefused) return error.exitCode;
  if (isRequestedHelp(error)) return SUCCESS_EXIT_CODE;
  const domain = domainExitCode(error);
  if (domain !== undefined) return domain;
  if (CliError.isCliError(error)) return EXIT_CODE.ValidationError;
  return Runtime.getErrorExitCode(error);
};

/**
 * Whether this failure was the CLI library's own, and therefore already
 * written.
 *
 * The library renders its parse and usage errors — and the help document
 * beneath them — before the failure ever leaves it. So the entry point must
 * NOT hand these to the shared renderer, which would staple a second,
 * differently-shaped line underneath. It takes only the exit code from them.
 */
export const isLibraryUsageError = (cause: Cause.Cause<unknown>): boolean =>
  CliError.isCliError(Cause.squash(cause));

/**
 * Whether this failure is a refusal the guard already wrote.
 *
 * It carries its verdict in the exit code and nothing in its message, because
 * the sentence went to stderr at the point of decision, where the register and
 * the streams were known.
 */
export const isGuardRefusal = (cause: Cause.Cause<unknown>): boolean =>
  Cause.squash(cause) instanceof GuardRefused;
