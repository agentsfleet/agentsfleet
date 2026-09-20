// Identifiers for an invocation rejected on argument shape — a missing
// positional, a missing or unknown option, a malformed value, an unknown
// command, or excess arguments.
//
// Two consumers share this module, which is why the literals live here and
// nowhere else (RULE UFS): program/entry/rejection.ts reformats the parser's
// rejections into the house shape, and errors/index.ts + errors/auth.ts build
// the same suggestion line onto a CliError. Before this module the prefix was
// declared twice, once in each errors file, and the two could drift without
// anything failing.
//
// Rejection is client-side: it happens before any request leaves the
// process, so these codes are NOT server UZ-* registry entries. They are the
// stable strings a `--json` consumer switches on (RULE JCL), so renaming one
// is a breaking change to the machine surface.

export const SUGGESTION_PREFIX = "\n  Suggestion: " as const;
export const USAGE_PREFIX = "usage: " as const;
export const CLI_NAME = "agentsfleet" as const;

// A group node and the root have no runnable usage line of their own, so
// their suggestion points at the command list instead.
export const HELP_HINT_PREFIX = "run `" as const;
export const HELP_FLAG = "--help" as const;
export const HELP_HINT_SUFFIX = "` for the command list" as const;

export const REJECTION_CODE = {
  missingArgument: "MISSING_ARGUMENT",
  missingOptionValue: "MISSING_OPTION_VALUE",
  missingRequiredOption: "MISSING_REQUIRED_OPTION",
  unknownCommand: "UNKNOWN_COMMAND",
  unknownOption: "UNKNOWN_OPTION",
  invalidArgument: "INVALID_ARGUMENT",
  excessArguments: "EXCESS_ARGUMENTS",
} as const;

export type RejectionCode = (typeof REJECTION_CODE)[keyof typeof REJECTION_CODE];
