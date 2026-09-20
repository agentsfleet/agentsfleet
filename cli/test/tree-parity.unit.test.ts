// The new tree declares the same surface as the one it replaces.
//
// A parser swap is only safe if nothing a person can type changes, so this
// diffs the `effect/unstable/cli` tree against the commander tree command for
// command. It is the test that has to pass BEFORE the entry point cuts over,
// and the one that gets deleted with commander afterwards — at which point
// `command-matrix-parity.unit.test.ts` becomes the anchor on its own.

import { describe, expect, test } from "bun:test";
import type { Command as CommanderCommand } from "commander";

import { buildProgram } from "../src/program/cli-tree.ts";
import { rootCommand } from "../src/program/tree/root.command.ts";
import type { CommandHandlerFn, Handlers } from "../src/program/cli-tree-types.ts";

const CLI_NAME = "agentsfleet";
// commander registers its own `help` verb on every group; the new tree serves
// help from a built-in that is not a command, so it is not a surface diff.
const BUILTIN_HELP_COMMAND = "help";

// Only the tree shape is under test, so the handlers merely have to exist.
const makeStubHandlers = (): Handlers => {
  const noop: CommandHandlerFn = async () => 0;
  return {
    login: noop, logout: noop, doctor: noop, whoami: noop,
    auth:      { status: noop },
    workspace: { create: noop, list: noop, use: noop, show: noop, secrets: noop, delete: noop },
    apiKey:    { create: noop, list: noop, revoke: noop, delete: noop },
    connector: { list: noop, status: noop },
    grant:     { list: noop, delete: noop },
    approvals: { list: noop, show: noop, approve: noop, deny: noop },
    schedule:  { add: noop, list: noop, update: noop, rm: noop, status: noop, sync: noop },
    tenant:    { provider: { show: noop, create: noop, delete: noop } },
    billing:   { show: noop },
    fleet: {
      library: noop, libraryAdd: noop, models: noop,
      install: noop, update: noop, list: noop, status: noop, stop: noop, resume: noop,
      kill: noop, delete: noop, logs: noop, events: noop, steer: noop,
      secret: { create: noop, update: noop, show: noop, list: noop, delete: noop },
    },
    memory: { list: noop, search: noop },
  } as Handlers;
};

const commanderPaths = (): string[] => {
  const program = buildProgram({
    handlers: makeStubHandlers(),
    version: "0.0.0",
    state: { exitCode: 0 },
  });
  const out: string[] = [];
  const walk = (cmd: CommanderCommand, prefix: ReadonlyArray<string>): void => {
    for (const sub of cmd.commands) {
      if (sub.name() === BUILTIN_HELP_COMMAND) continue;
      const path = [...prefix, sub.name()];
      out.push(path.join(" "));
      walk(sub, path);
    }
  };
  walk(program, []);
  return out.sort();
};

// `subcommands` on an effect/unstable/cli command is an array of GROUPS, each
// carrying its own `commands` array — not the commands themselves. Walking it
// as a flat list silently reports one child and looks like an empty tree.
interface TreeGroup {
  readonly commands: ReadonlyArray<TreeNode>;
}
interface TreeNode {
  readonly name: string;
  readonly subcommands?: ReadonlyArray<TreeGroup>;
}

const childrenOf = (node: TreeNode): ReadonlyArray<TreeNode> =>
  (node.subcommands ?? []).flatMap((group) => group.commands);

const effectPaths = (): string[] => {
  const walk = (node: TreeNode, prefix: ReadonlyArray<string>): string[] => {
    const path = [...prefix, node.name];
    return [path.join(" "), ...childrenOf(node).flatMap((child) => walk(child, path))];
  };
  return childrenOf(rootCommand as unknown as TreeNode)
    .flatMap((child) => walk(child, []))
    .sort();
};

describe("tree parity — the effect/unstable/cli tree against commander", () => {
  test("declares exactly the same command paths", () => {
    expect(effectPaths()).toEqual(commanderPaths());
  });

  test("the root is named for the binary", () => {
    expect((rootCommand as unknown as TreeNode).name).toBe(CLI_NAME);
  });

  test("every command carries a description, so help is never blank", () => {
    const described = (node: TreeNode): string[] => {
      const self = (node as unknown as { description?: string }).description;
      const missing = self === undefined || self.length === 0 ? [node.name] : [];
      return [...missing, ...childrenOf(node).flatMap(described)];
    };
    expect(childrenOf(rootCommand as unknown as TreeNode).flatMap(described)).toEqual([]);
  });
});
