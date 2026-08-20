// The matrix fixture is only a single source of truth if it cannot fall
// behind the command tree. These tests walk the built commander program and
// diff what it actually declares against what the fixture enumerates, so a
// command that lands with a required argument and no matrix row fails here
// rather than going quietly unswept.
//
// This is the invariant that would have caught the gap M171 was opened for:
// `events` and `steer` both declared <fleet_id> while the fixture listed
// neither, and nothing failed.

import { describe, expect, test } from "bun:test";
import type { Command } from "commander";

import { buildProgram } from "../src/program/cli-tree.ts";
import {
  GROUP_NODES,
  REQUIRES_POSITIONAL_ARG,
} from "./acceptance/fixtures/command-matrix.ts";
import type { CommandHandlerFn, Handlers } from "../src/program/cli-tree-types.ts";

const CLI_NAME = "agentsfleet";
const BUILTIN_HELP_COMMAND = "help";

// The tree shape is what is under test, so the handlers only have to exist.
function makeStubHandlers(): Handlers {
  const noop: CommandHandlerFn = async () => 0;
  return {
    login: noop, logout: noop, doctor: noop,
    auth:      { status: noop },
    workspace: { create: noop, list: noop, use: noop, show: noop, secrets: noop, delete: noop },
    apiKey:    { create: noop, list: noop, revoke: noop, delete: noop },
    connector: { list: noop, status: noop },
    grant:     { list: noop, delete: noop },
    schedule:  { add: noop, list: noop, update: noop, rm: noop, status: noop, sync: noop },
    tenant:    { provider: { show: noop, create: noop, delete: noop } },
    billing:   { show: noop },
    fleet: {
      library: noop, models: noop,
      install: noop, update: noop, list: noop, status: noop, stop: noop, resume: noop,
      kill: noop, delete: noop, logs: noop, events: noop, steer: noop,
      secret: { create: noop, update: noop, show: noop, list: noop, delete: noop },
    },
    memory: { list: noop, search: noop },
  };
}

function pathOf(cmd: Command): string[] {
  const segments: string[] = [];
  for (let node: Command | null = cmd; node; node = node.parent) {
    const name = node.name();
    if (name && name !== CLI_NAME) segments.unshift(name);
  }
  return segments;
}

function walk(cmd: Command, visit: (c: Command) => void): void {
  for (const sub of cmd.commands) {
    visit(sub);
    walk(sub, visit);
  }
}

function builtProgram(): Command {
  return buildProgram({
    handlers: makeStubHandlers(),
    version: "0.0.0",
    state: { exitCode: 0 },
  });
}

// commander marks a declared positional required when it is spelled <name>.
function requiredArgNames(cmd: Command): string[] {
  const args = (cmd as unknown as {
    registeredArguments?: ReadonlyArray<{ required?: boolean; name(): string }>;
  }).registeredArguments ?? [];
  return args.filter((a) => a.required === true).map((a) => a.name());
}

describe("command matrix parity — required positionals", () => {
  test("every command declaring a required positional has a matrix row", () => {
    const declared: string[] = [];
    walk(builtProgram(), (cmd) => {
      if (requiredArgNames(cmd).length > 0) declared.push(pathOf(cmd).join(" "));
    });
    const covered = new Set(REQUIRES_POSITIONAL_ARG.map((r) => r.args.join(" ")));
    const missing = declared.filter((d) => !covered.has(d)).sort();
    expect(missing).toEqual([]);
  });

  test("every matrix row names an argument the command actually declares", () => {
    const byPath = new Map<string, string[]>();
    walk(builtProgram(), (cmd) => {
      byPath.set(pathOf(cmd).join(" "), requiredArgNames(cmd));
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
    walk(builtProgram(), (cmd) => {
      // `help` is commander's built-in and carries no house help body.
      if (cmd.commands.length > 0 && cmd.name() !== BUILTIN_HELP_COMMAND) {
        groups.push(pathOf(cmd).join(" "));
      }
    });
    const covered = new Set(GROUP_NODES.map((g) => g.join(" ")));
    const missing = groups.filter((g) => !covered.has(g)).sort();
    expect(missing).toEqual([]);
  });
});
