// Identifiers for an invocation rejected on argument shape — a missing
// positional, a missing or unknown option, a malformed value, an unknown
// command, or excess arguments.
//
// Two consumers share this module, which is why the literals live here and
// nowhere else (RULE UFS): lib/commander-error-render.ts reformats
// commander's parse-stage rejections, and errors/index.ts + errors/auth.ts
// build the same suggestion line onto a CliError. Before this module the
// prefix was declared twice, once in each errors file, and the two could
// drift without anything failing.
//
// Rejection is client-side: it happens before any request leaves the
// process, so these codes are NOT server UZ-* registry entries. They are the
// stable strings a `--json` consumer switches on (RULE JCL), so renaming one
// is a breaking change to the machine surface.

export const SUGGESTION_PREFIX = "\n  Suggestion: " as const;
export const USAGE_PREFIX = "usage: " as const;
export const CLI_NAME = "agentsfleet" as const;

// Commander prefixes its own rejection text with this; the house renderer
// strips it so the detail reads as one sentence under the ✕ glyph.
export const COMMANDER_ERROR_PREFIX = "error: " as const;

// A group node and the root have no runnable usage line of their own, so
// their suggestion points at the command list instead.
export const HELP_HINT_PREFIX = "run `" as const;
export const HELP_FLAG = "--help" as const;
export const HELP_HINT_SUFFIX = "` for the command list" as const;

// commander's showHelpAfterError text. It arrives through the same output
// hook as the rejection itself and is dropped, because the Suggestion line
// replaces it with something the operator can actually run.
export const COMMANDER_HELP_HINT = "(use --help for usage)" as const;

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

// commander.* error code -> the stable code a --json consumer sees. The key
// set is the same one cli.ts treats as a usage rejection; a commander code
// absent here is not an invocation error and keeps commander's own exit.
export const COMMANDER_CODE_TO_REJECTION: Readonly<Record<string, RejectionCode>> = {
  "commander.missingArgument": REJECTION_CODE.missingArgument,
  "commander.optionMissingArgument": REJECTION_CODE.missingOptionValue,
  "commander.missingMandatoryOptionValue": REJECTION_CODE.missingRequiredOption,
  "commander.unknownCommand": REJECTION_CODE.unknownCommand,
  "commander.unknownOption": REJECTION_CODE.unknownOption,
  "commander.invalidArgument": REJECTION_CODE.invalidArgument,
  "commander.excessArguments": REJECTION_CODE.excessArguments,
};
