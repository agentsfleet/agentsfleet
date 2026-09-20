// `agentsfleet help [command]` and `agentsfleet [command] --help` are two
// spellings of one request. The previous parser carried `help` as a built-in
// command; this one does not, so the entry rewrites it. These tests exist to
// stop the two drifting: a rewrite that lost its argument would silently
// answer the root's help for every command anyone asked about.

import { describe, expect, test } from "bun:test";

import { runCli } from "../src/cli.ts";

const capture = (): { read: () => string; stream: { write(c: string): boolean; isTTY: boolean } } => {
  const chunks: string[] = [];
  return {
    read: () => chunks.join(""),
    stream: { write: (c) => { chunks.push(c); return true; }, isTTY: false },
  };
};

const help = async (argv: ReadonlyArray<string>): Promise<{ code: number; out: string }> => {
  const out = capture();
  const err = capture();
  const code = await runCli([...argv], {
    stdout: out.stream,
    stderr: err.stream,
    env: { ...process.env, NO_COLOR: "1" },
  });
  return { code, out: out.read() };
};

describe("the help command form", () => {
  test("bare `agentsfleet` answers the root help at exit 0", async () => {
    const { code, out } = await help([]);
    expect(code).toBe(0);
    expect(out).toContain("agentsfleet");
  });

  test("`help` answers the same body as `--help`", async () => {
    const viaCommand = await help(["help"]);
    const viaFlag = await help(["--help"]);
    expect(viaCommand.code).toBe(0);
    expect(viaCommand.out).toBe(viaFlag.out);
  });

  test("`help <group>` answers that group, not the root", async () => {
    const viaCommand = await help(["help", "workspace"]);
    const viaFlag = await help(["workspace", "--help"]);
    expect(viaCommand.code).toBe(0);
    expect(viaCommand.out).toBe(viaFlag.out);
    expect(viaCommand.out).toContain("Manage workspaces");
  });

  test("`help <group> <verb>` reaches the verb's own help", async () => {
    const viaCommand = await help(["help", "workspace", "create"]);
    const viaFlag = await help(["workspace", "create", "--help"]);
    expect(viaCommand.out).toBe(viaFlag.out);
  });

  // `help` is only special as the FIRST token: a command that takes an
  // argument spelled `help` must still receive it.
  test("a later `help` token is an argument, not the help request", async () => {
    const viaCommand = await help(["workspace", "help"]);
    const rootHelp = await help(["--help"]);
    expect(viaCommand.out).not.toBe(rootHelp.out);
  });
});
