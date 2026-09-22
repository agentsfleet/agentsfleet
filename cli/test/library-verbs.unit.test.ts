// `library` spoke the same private dialect `schedule` did: add where every
// other collection says create, and remove where the rest says delete.
//
// `remove` never shipped — it arrives and is renamed inside one milestone — but
// `add` did, in 0.49.0, and no alias is kept for it. A compatibility verb is
// what the rules forbid at this version, so a retired spelling answers as the
// unknown subcommand it now is.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";

const RENAMED = [
  ["add", "create"],
  ["remove", "delete"],
] as const;

const help = async (argv: ReadonlyArray<string>) => {
  const out = bufferStream();
  const err = bufferStream();
  const code = await runCli([...argv], { stdout: out.stream, stderr: err.stream, env: cliEnv({}) });
  return { code, stdout: out.read(), stderr: err.read() };
};

describe("the library verbs match the rest of the surface", () => {
  test.each(RENAMED.map(([, to]) => to))("library %s is a real subcommand", async (verb) => {
    const { code, stdout } = await help(["library", verb, "--help"]);
    expect(code).toBe(0);
    expect(stdout).toContain(verb);
  });

  test.each(RENAMED.map(([from]) => from))(
    "the retired spelling %s is refused, pointing at the group's list",
    async (verb) => {
      const { code, stderr } = await help(["library", verb]);
      expect(code).not.toBe(0);
      expect(stderr).toContain(verb);
      expect(stderr).toContain("agentsfleet library --help");
    },
  );

  test("no compatibility alias survived either spelling", async () => {
    const { stdout } = await help(["library", "--help"]);
    for (const [from, to] of RENAMED) {
      expect(stdout).toContain(to);
      // The retired verb must be gone from the listing, not merely deprioritised.
      expect(stdout).not.toMatch(new RegExp(`^\\s+${from}\\s`, "m"));
    }
  });
});

describe("no usage line names a verb the parser refuses", () => {
  // The schedule rename moved the verbs and left the usage lines behind, so a
  // bad input answered with `usage: agentsfleet schedule rm ...` — a sentence
  // naming the one spelling the parser had just stopped accepting. A
  // suggestion that cannot be typed is worse than none.
  //
  // Read from source rather than driven through the handler: every one of
  // these sentences sits behind a credential check, so invoking the verb in a
  // signed-out fixture answers with an auth refusal and grades nothing.
  // A plain array, not `as const`: `test.each` over a readonly tuple widens
  // its callback argument to `unknown`.
  const MODULES: string[] = [
    "fleet_library.ts",
    "fleet_library_create.ts",
    "fleet_library_delete.ts",
    "fleet_library_list.ts",
  ];

  test.each(MODULES)("%s names no retired library verb", async (module) => {
    const { readFileSync } = await import("node:fs");
    const { join } = await import("node:path");
    const text = readFileSync(join(import.meta.dir, "..", "src", "commands", module), "utf8");
    for (const [retired] of RENAMED) {
      expect(text).not.toContain(`agentsfleet library ${retired}`);
    }
  });

  test("the dashboard's empty state names a verb the CLI accepts", async () => {
    // The hint an operator reads when the collection is empty is where a
    // retired spelling survives longest, because no parser refuses a string,
    // and this copy lives in the app rather than the CLI.
    const { readFileSync } = await import("node:fs");
    const { join } = await import("node:path");
    const copy = join(
      import.meta.dir, "..", "..", "ui", "packages", "app", "app",
      "(dashboard)", "w", "[workspaceId]", "library", "copy.ts",
    );
    const text = readFileSync(copy, "utf8");
    for (const [retired] of RENAMED) {
      expect(text).not.toContain(`agentsfleet library ${retired}`);
    }
    expect(text).toContain("agentsfleet library create");
  });
});
