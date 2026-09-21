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

  // `help` is only special as the first NON-FLAG token: a command that takes
  // an argument spelled `help` must still receive it.
  test("a later `help` token is an argument, not the help request", async () => {
    const viaCommand = await help(["workspace", "help"]);
    const rootHelp = await help(["--help"]);
    expect(viaCommand.out).not.toBe(rootHelp.out);
  });

  // A global flag before the verb is the ordinary shape for `--api` and
  // `--json`. Recognising `help` at position zero alone meant these failed as
  // an unknown command while the same line without the flag printed the
  // document — a difference nobody would predict from the two invocations.
  test("a global flag before `help` does not hide it", async () => {
    const bare = await help(["help", "workspace", "create"]);
    for (const leading of [["--json"], ["--api", "https://api.test.local"]]) {
      const prefixed = await help([...leading, "help", "workspace", "create"]);
      expect(prefixed.code, `${leading.join(" ")} help must not fail`).toBe(bare.code);
      expect(prefixed.out, `${leading.join(" ")} help must print the same body`).toBe(bare.out);
    }
  });

  // `--api=<url>` carries its own value, so the token after it is the verb.
  // Skipping one token per flag AND one for a value would eat `help` here.
  test("an inline flag value does not consume the `help` token", async () => {
    const bare = await help(["help", "workspace", "create"]);
    const inline = await help(["--api=https://api.test.local", "help", "workspace", "create"]);
    expect(inline.out).toBe(bare.out);
  });
});
