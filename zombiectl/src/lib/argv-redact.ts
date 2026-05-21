// Detect `--token <value>` (or `--token=<value>`) in process argv and
// return the operator-facing warning text. Pure function — no stderr, no
// process.env, no environment introspection — so it's trivially testable
// and safe to call before any logging/output service is wired.
//
// The CLI's token-resolution chain is creds.json → ZOMBIE_TOKEN. `--token`
// is NOT in that chain today — it isn't accepted as a global
// flag and is not parsed by commander. Operators sometimes pass it
// expecting it'll work, and bash history captures the secret verbatim
// either way. We surface the warning so the next attempt routes through
// the env-var path instead.
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
  "warning: --token leaks into shell history and process lists; prefer ZOMBIE_TOKEN.";

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
