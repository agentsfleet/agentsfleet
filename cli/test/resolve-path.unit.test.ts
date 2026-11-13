// The command path has to be right before the parser runs, because the layer
// the tree executes in is built from it — a wrong path mislabels every span
// and every analytics row for that invocation.

import { describe, expect, test } from "bun:test";

import { buildCommandTree } from "../src/program/tree/index.ts";

import {
  resolveCommandPath,
  type CommandNode,
} from "../src/program/tree/resolve-path.ts";

const rootCommand = buildCommandTree();

const root = rootCommand as unknown as CommandNode;
const resolve = (argv: ReadonlyArray<string>): ReadonlyArray<string> =>
  resolveCommandPath(root, argv);

const VALID_ID = "0192a3b4-c5d6-7e8f-9012-345678901234";

describe("resolveCommandPath", () => {
  test("a bare verb resolves to itself", () => {
    expect(resolve(["whoami"])).toEqual(["whoami"]);
  });

  test("a nested verb resolves to its full path", () => {
    expect(resolve(["models", "current"])).toEqual(["models", "current"]);
  });

  test("flags before the verb do not hide it", () => {
    expect(resolve(["--json", "fleet", "grants", "list"])).toEqual(["fleet", "grants", "list"]);
  });

  test("an `=` flag carries its own value and consumes nothing after it", () => {
    expect(resolve(["--api=https://example.test", "fleet", "list"])).toEqual(["fleet", "list"]);
  });

  // The trap this walk exists to avoid: a flag VALUE that is spelled like a
  // command. `--workspace list` must not resolve `list` as the command.
  test("a separated flag swallows its value, even when the value names a command", () => {
    expect(resolve(["connections", "--workspace", "list"])).toEqual(["connections"]);
  });

  test("positionals after the verb are not commands", () => {
    expect(resolve(["fleet", "archive", VALID_ID])).toEqual(["fleet", "archive"]);
  });

  test("an unknown verb resolves to nothing", () => {
    expect(resolve(["definitely-not-a-real-command"])).toEqual([]);
  });

  test("an unknown subcommand stops at the group that did resolve", () => {
    expect(resolve(["workspace", "frobnicate"])).toEqual(["workspace"]);
  });

  test("empty argv resolves to nothing", () => {
    expect(resolve([])).toEqual([]);
  });

  test("everything after `--` is an argument, never a command", () => {
    expect(resolve(["fleet", "steer", "--", "list"])).toEqual(["fleet", "steer"]);
  });
});
