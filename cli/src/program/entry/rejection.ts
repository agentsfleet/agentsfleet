// A rejected invocation, in this CLI's own shape.
//
// The library states its parse failures plainly — "Missing argument", and a
// help document underneath. This CLI has always answered in a fixed two-line
// shape instead:
//
//     ✕ error: <what was wrong>
//       Suggestion: <what to run instead>
//
// and under `--json`, an envelope carrying a stable code. Both halves are
// load-bearing. The Suggestion is the half a person acts on, and it must name
// a fix rather than restate the fault — the acceptance suite asserts the two
// lines differ, because a suggestion that repeats the detail is decoration.
// The codes are a machine contract (`constants/rejection.ts`, RULE JCL): a
// `--json` consumer switches on them, so renaming one breaks that consumer.
//
// The command path comes from argv rather than the error, because only some
// variants carry one — `MissingArgument` names the argument and nothing else,
// and a usage line without the command is no use to anybody.

import { CliError } from "effect/unstable/cli";
import {
  CLI_NAME,
  HELP_FLAG,
  HELP_HINT_PREFIX,
  HELP_HINT_SUFFIX,
  REJECTION_CODE,
  USAGE_PREFIX,
  type RejectionCode,
} from "../../constants/rejection.ts";
import { childrenOf, resolveCommandPath, type CommandNode } from "../tree/resolve-path.ts";

const OPTIONS_PLACEHOLDER = "[options]" as const;
const OPTIONAL_TAG = "Optional" as const;
const FLAG_KIND = "flag" as const;
const FLAG_PREFIX = "--" as const;

export interface HouseRejection {
  readonly code: RejectionCode;
  readonly detail: string;
  readonly suggestion: string;
}

// Every variant that means "the invocation was wrong". `ShowHelp` is absent
// because a help document is not a rejection, and `DuplicateOption` is absent
// because it fires at construction — a user cannot provoke it.
const CODE_BY_TAG: Readonly<Record<string, RejectionCode>> = {
  MissingArgument: REJECTION_CODE.missingArgument,
  MissingOption: REJECTION_CODE.missingRequiredOption,
  UnrecognizedOption: REJECTION_CODE.unknownOption,
  UnknownSubcommand: REJECTION_CODE.unknownCommand,
  InvalidValue: REJECTION_CODE.invalidArgument,
  UnexpectedArgument: REJECTION_CODE.excessArguments,
};

interface ArgumentNode extends CommandNode {
  readonly config?: { readonly arguments?: ReadonlyArray<unknown> };
}

interface MaybeWrapped {
  readonly _tag?: string;
  readonly param?: MaybeWrapped;
  readonly name?: string;
}

// `<fleet_id>` for a required positional, `[message]` for an optional one —
// the same spelling the help document uses, so the usage line a person is
// shown matches the one they read.
const spellArgument = (param: unknown): string => {
  let current = param as MaybeWrapped;
  let optional = false;
  while (current.name === undefined) {
    if (current._tag === OPTIONAL_TAG) optional = true;
    if (current.param === undefined) return "";
    current = current.param;
  }
  return optional ? `[${current.name}]` : `<${current.name}>`;
};

const nodeAt = (root: CommandNode, path: ReadonlyArray<string>): CommandNode | null => {
  let node = root;
  for (const name of path) {
    const next = childrenOf(node).find((child) => child.name === name);
    if (!next) return null;
    node = next;
  }
  return node;
};

/**
 * What to run instead.
 *
 * A leaf gets its own usage line, which is a command someone can type. A group
 * or the root has no runnable form of its own, so it points at the command
 * list — telling somebody to run `agentsfleet workspace` when that is what
 * just failed would be a loop.
 */
const suggestionFor = (root: CommandNode, path: ReadonlyArray<string>): string => {
  const node = path.length > 0 ? nodeAt(root, path) : root;
  if (node === null || childrenOf(node).length > 0 || path.length === 0) {
    // The hint names the group that was actually reached: `connector pogo`
    // sends someone to `agentsfleet connector --help`, which lists the verbs
    // they were choosing between, not the root list they have to search again.
    const scope = [CLI_NAME, ...path].join(" ");
    return `${HELP_HINT_PREFIX}${scope} ${HELP_FLAG}${HELP_HINT_SUFFIX}`;
  }
  const args = ((node as ArgumentNode).config?.arguments ?? [])
    .map(spellArgument)
    .filter((spelling) => spelling.length > 0);
  return [USAGE_PREFIX + CLI_NAME, ...path, OPTIONS_PLACEHOLDER, ...args].join(" ");
};

/**
 * The rejection inside a failure, if there is one.
 *
 * A parse failure does not arrive as itself: the library wraps it in
 * `ShowHelp` so the help document renders beneath it, and the real cause sits
 * in that wrapper's `errors` array. Reading only the outer tag finds
 * `ShowHelp` every time and concludes nothing was rejected — which is how
 * every usage error would silently lose its house shape.
 *
 * An empty `errors` array is help somebody asked for, and not a rejection.
 */
const unwrapShowHelp = (error: unknown): CliError.CliError | null => {
  if (!CliError.isCliError(error)) return null;
  if (!(error instanceof CliError.ShowHelp)) return error;
  const [first] = error.errors;
  return first ?? null;
};

/**
 * The refusal sentence, in this repository's spelling rather than the library's.
 *
 * The library composes `Invalid value for argument <fleet_id>: "x". Expected:
 * <what the filter said>`, which reads "Expected: expected uuidv7 format" once
 * the filter supplies a sentence of its own, and drops the `invalid <name>:`
 * stem every other refusal in this CLI opens with. `InvalidValue` carries
 * `option`, `value`, `expected` and `kind` as fields, so the house sentence is
 * rebuilt from those rather than parsed back out of the composed one.
 *
 * The stem matches `validateRequiredId` in `lib/id.ts`, and that is the point:
 * the parser refuses a malformed id before a handler runs, the handler refuses
 * one that never passed through a flag, and a person who hits either path reads
 * the same sentence. Two spellings of one rule is what this replaces.
 *
 * Flags and positionals take the SAME shape. Splitting them by kind would
 * rebuild the inconsistency one layer down, and nothing pins the library's
 * flag wording — `options-metavar.spec.ts` asserts the stem alone, which is
 * the rule an operator has to satisfy.
 *
 * The offending value is kept. A flag buried in a long invocation is the case
 * where "which one was wrong" is not obvious from the line just typed.
 *
 * `kind` is read for ONE thing: a flag is named the way it was typed. The
 * field carries the bare word, so `--` is restored — `invalid fleet:` sends
 * someone looking for a positional they never passed.
 */
const houseDetail = (rejected: CliError.CliError): string => {
  if (rejected instanceof CliError.InvalidValue) {
    const name = rejected.kind === FLAG_KIND ? `${FLAG_PREFIX}${rejected.option}` : rejected.option;
    return `invalid ${name}: ${rejected.expected} (got ${JSON.stringify(rejected.value)})`;
  }
  return rejected.message;
};

/**
 * The house shape for a library parse failure, or null if it is not one.
 *
 * Null means the caller should leave the error alone: a help document, or
 * anything this CLI has no opinion about.
 */
export const houseRejection = (
  error: unknown,
  root: CommandNode,
  argv: ReadonlyArray<string>,
): HouseRejection | null => {
  const rejected = unwrapShowHelp(error);
  if (rejected === null) return null;
  const code = CODE_BY_TAG[rejected._tag];
  if (code === undefined) return null;
  return {
    code,
    detail: houseDetail(rejected),
    suggestion: suggestionFor(root, resolveCommandPath(root, argv)),
  };
};
