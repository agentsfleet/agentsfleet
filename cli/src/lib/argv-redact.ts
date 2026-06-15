// Detect `--token <value>` (or `--token=<value>`) in process argv and
// return the operator-facing warning text. Pure function — no stderr, no
// process.env, no environment introspection — so it's trivially testable
// and safe to call before any logging/output service is wired.
//
// `agentsfleet login --token <pat>` is an accepted, non-interactive auth
// path — but the secret still lands in shell history and process lists.
// We emit this caveat whenever `--token` appears in argv, then let the
// command proceed (the flag works); the message just nudges operators
// toward AGENTSFLEET_TOKEN or piped stdin, which don't leak.
//
// Detection rules (POSIX-faithful):
//   - `--` ends the option-scanning window (anything after it is a
//     positional, including a literal `--token`).
//   - `--token=<value>` matches when `<value>` is non-empty.
//   - `--token <value>` matches when the next argv slot exists, is
//     non-empty, and does not itself start with `--`. The flag-without-
//     value spelling (`--token` at end-of-argv or followed by another
//     flag) is harmless — there's nothing for the shell to capture.

export const TOKEN_LEAK_WARNING =
  "warning: --token leaks into shell history and process lists; prefer AGENTSFLEET_TOKEN.";

const TOKEN_FLAG = "--token";
const TOKEN_EQ_PREFIX = "--token=";
const END_OF_OPTIONS = "--";

const hasInlineValue = (arg: string): boolean =>
  arg.startsWith(TOKEN_EQ_PREFIX) && arg.length > TOKEN_EQ_PREFIX.length;

const hasFollowingValue = (argv: readonly string[], i: number): boolean => {
  const next = argv[i + 1];
  return typeof next === "string" && next.length > 0 && !next.startsWith("--");
};

export function detectTokenInArgv(argv: readonly string[]): string | null {
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === undefined) continue;
    if (a === END_OF_OPTIONS) return null;
    if (hasInlineValue(a)) return TOKEN_LEAK_WARNING;
    if (a === TOKEN_FLAG && hasFollowingValue(argv, i)) return TOKEN_LEAK_WARNING;
  }
  return null;
}
