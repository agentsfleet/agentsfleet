// Which command an argv names, answered before the parser runs.
//
// The layer the tree executes in has to be built first, and it carries the
// command path — the span name, the analytics label, `cli.grant.list`. So the
// path is resolved by walking the tree against argv rather than by asking the
// parser, which has not run yet and cannot until the layer exists.
//
// This is a name walk, not a parse. It stops at the first token that is not a
// child of the current node, which is the right answer for both halves of the
// job: a valid invocation yields its full path, and an invalid one yields the
// deepest command that did resolve — the one whose help a person is about to
// be shown.

import { Param } from "effect/unstable/cli";

const FLAG_PREFIX = "--" as const;
const SHORT_FLAG_PREFIX = "-" as const;
const END_OF_FLAGS = "--" as const;
const BOOLEAN_PRIMITIVE = "Boolean" as const;

export interface CommandNode {
  readonly name: string;
  readonly subcommands?: ReadonlyArray<{ readonly commands: ReadonlyArray<CommandNode> }>;
}

/**
 * A command's children.
 *
 * `subcommands` is an array of GROUPS, each holding its own `commands` array —
 * not the commands themselves. Reading it as a flat list reports one child and
 * makes a twenty-eight-command tree look empty.
 */
export const childrenOf = (node: CommandNode): ReadonlyArray<CommandNode> =>
  (node.subcommands ?? []).flatMap((group) => group.commands);

interface ConfiguredNode extends CommandNode {
  readonly config?: { readonly flags?: ReadonlyArray<Param.Any> };
}

interface WrappedParam {
  readonly param: Param.Any;
}

const isWrapped = (param: Param.Any): param is Param.Any & WrappedParam =>
  "param" in param;

/**
 * The flag underneath its combinators.
 *
 * `Flag.optional` and friends WRAP the flag they decorate rather than
 * replacing it, so the leaf carrying `primitiveType` sits a few layers down
 * behind `param`. The walk ends on `Param.isSingle` — the public guard — and
 * fails closed on a variant it does not recognise, the same shape the
 * reference CLI uses in `command-internal/param-introspection.ts`. The
 * library's own `extractSingleParams` is `@internal` and absent from the
 * published types.
 */
const unwrap = (flag: Param.Any): Param.Single<Param.ParamKind, unknown> | undefined => {
  let current = flag;
  while (!Param.isSingle(current)) {
    if (!isWrapped(current)) return undefined;
    current = current.param;
  }
  return current;
};

/**
 * Every flag in the tree that takes no value.
 *
 * Read off the tree rather than listed here, because a list is a second place
 * to remember a flag and the one that gets forgotten. It matters because a
 * flag's value can be spelled exactly like a command — `--workspace list` —
 * so the walk has to know which flags swallow the token after them and which,
 * like `--json`, do not.
 */
export const booleanFlagNames = (root: CommandNode): ReadonlySet<string> => {
  const names = new Set<string>();
  const visit = (node: CommandNode): void => {
    for (const flag of (node as ConfiguredNode).config?.flags ?? []) {
      const single = unwrap(flag);
      if (single?.primitiveType._tag === BOOLEAN_PRIMITIVE) names.add(single.name);
    }
    for (const child of childrenOf(node)) visit(child);
  };
  visit(root);
  return names;
};

export const resolveCommandPath = (
  root: CommandNode,
  argv: ReadonlyArray<string>,
  valueless: ReadonlySet<string> = booleanFlagNames(root),
): ReadonlyArray<string> => {
  const path: string[] = [];
  let node = root;
  let skipNext = false;

  for (const token of argv) {
    if (token === END_OF_FLAGS) break;
    if (skipNext) {
      skipNext = false;
      continue;
    }
    if (token.startsWith(SHORT_FLAG_PREFIX)) {
      // `--flag=value` carries its own value. A bare `--flag` swallows the
      // next token unless the tree says it is a boolean; an unknown flag is
      // assumed to take one, since the parser is about to reject it anyway
      // and the walk only decides which help to print.
      skipNext =
        token.startsWith(FLAG_PREFIX) &&
        !token.includes("=") &&
        !valueless.has(token.slice(FLAG_PREFIX.length));
      continue;
    }
    const match = childrenOf(node).find((child) => child.name === token);
    if (!match) break;
    path.push(match.name);
    node = match;
  }

  return path;
};
