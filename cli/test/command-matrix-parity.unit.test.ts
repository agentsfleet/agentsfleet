// The matrix fixture is only a single source of truth if it cannot fall behind
// the command tree. These tests walk the real tree and diff what it actually
// declares against what the fixture enumerates, so a command that lands with a
// required argument and no matrix row fails here rather than going unswept.
//
// This is the invariant M171 was opened for: `events` and `steer` both declared
// <fleet_id> while the fixture listed neither, and nothing failed.

import { describe, expect, test } from "bun:test";
import { Param } from "effect/unstable/cli";

import { rootCommand } from "../src/program/tree/root.command.ts";
import { childrenOf, type CommandNode } from "../src/program/tree/resolve-path.ts";
import {
  ACTION_GROUP_NODES,
  GROUP_NODES,
  REQUIRES_POSITIONAL_ARG,
} from "./acceptance/fixtures/command-matrix.ts";

const OPTIONAL_TAG = "Optional";

interface ArgumentNode extends CommandNode {
  readonly config?: { readonly arguments?: ReadonlyArray<Param.Any> };
}

const tree = rootCommand as unknown as ArgumentNode;

// A positional is optional when one of the combinators wrapping it says so —
// `[message]` rather than `<fleet_id>`. Walking to the Single leaf and
// recording whether an `Optional` was crossed on the way is how the spelling
// is recovered, since the leaf itself carries only the name.
function requiredArgNames(node: ArgumentNode): ReadonlyArray<string> {
  const names: string[] = [];
  for (const argument of node.config?.arguments ?? []) {
    let current: Param.Any = argument;
    let optional = false;
    while (!Param.isSingle(current)) {
      if ((current as { readonly _tag?: string })._tag === OPTIONAL_TAG) optional = true;
      if (!("param" in current)) break;
      current = (current as { readonly param: Param.Any }).param;
    }
    if (!optional && Param.isSingle(current)) names.push(current.name);
  }
  return names;
}

function walk(
  node: ArgumentNode,
  path: ReadonlyArray<string>,
  visit: (node: ArgumentNode, path: ReadonlyArray<string>) => void,
): void {
  for (const child of childrenOf(node)) {
    const childPath = [...path, child.name];
    visit(child as ArgumentNode, childPath);
    walk(child as ArgumentNode, childPath, visit);
  }
}

describe("command matrix parity — required positionals", () => {
  test("every command declaring a required positional has a matrix row", () => {
    const declared: string[] = [];
    walk(tree, [], (node, path) => {
      if (requiredArgNames(node).length > 0) declared.push(path.join(" "));
    });
    const covered = new Set(REQUIRES_POSITIONAL_ARG.map((r) => r.args.join(" ")));
    expect(declared.filter((d) => !covered.has(d)).sort()).toEqual([]);
  });

  test("every matrix row names an argument the command actually declares", () => {
    const byPath = new Map<string, ReadonlyArray<string>>();
    walk(tree, [], (node, path) => {
      byPath.set(path.join(" "), requiredArgNames(node));
    });
    const wrong = REQUIRES_POSITIONAL_ARG.filter((row) => {
      const names = byPath.get(row.args.join(" "));
      return !names || !names.includes(row.missingArgName);
    }).map((row) => `${row.args.join(" ")} -> ${row.missingArgName}`);
    expect(wrong).toEqual([]);
  });
});

describe("command matrix parity — group nodes", () => {
  test("every command owning subcommands has a group-node row", () => {
    const groups: string[] = [];
    walk(tree, [], (node, path) => {
      if (childrenOf(node).length > 0) groups.push(path.join(" "));
    });
    // Either table covers a group: one asserts bare-invocation help, the other
    // records that bare invocation runs the command instead.
    const covered = new Set(
      [...GROUP_NODES, ...ACTION_GROUP_NODES].map((g) => g.join(" ")),
    );
    expect(groups.filter((g) => !covered.has(g)).sort()).toEqual([]);
  });
});
