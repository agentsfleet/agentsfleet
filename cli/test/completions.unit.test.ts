// The two global flags this CLI advertises but never covered.
//
// `--completions` emitted thousands of lines that nothing ever parsed, so a
// change to the command tree could have shipped a script the shell rejects
// and no test would have noticed. `--wizard` is the opposite problem: it
// works, this repository never designed it, and the help offered it anyway.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { commandPresent } from "./helpers-external-commands.ts";

const SHELLS = ["bash", "zsh", "fish", "sh"] as const;
// The two whose own parser can grade what we emit — where the box carries it.
// `zsh` is the standard shell on macOS and absent from GitHub's
// `ubuntu-latest`, so spawning it unconditionally fails the lane with
// `Executable not found in $PATH` rather than with anything about our script.
// `bash` is everywhere, and is asserted present rather than probed: a box
// without it is broken, which `external-commands.unit.test.ts` says out loud.
const PARSEABLE = ["bash", "zsh"] as const;
const ALWAYS_PARSEABLE = "bash" as const;

const emit = async (shell: string): Promise<{ code: number; text: string }> => {
  const out = bufferStream();
  const err = bufferStream();
  const code = await runCli(["--completions", shell], {
    stdout: out.stream,
    stderr: err.stream,
    env: cliEnv({}),
  });
  return { code, text: out.read() };
};

describe("completions emit a script each shell parses", () => {
  test.each([...SHELLS])("%s emits a non-empty script at exit 0", async (shell) => {
    const { code, text } = await emit(shell);
    expect(code).toBe(0);
    expect(text.length).toBeGreaterThan(0);
    expect(text).toContain("agentsfleet");
  });

  for (const shell of PARSEABLE) {
    const present = commandPresent(shell);
    // Skipped rather than failed, and only for a shell this box does not
    // carry: the claim is that the shell's own parser accepts our script, and
    // there is no parser to ask. The case below keeps the skip from quietly
    // emptying this describe.
    test.skipIf(!present)(`${shell} accepts what we emit for it`, async () => {
      const { text } = await emit(shell);
      // -n is parse-only: the shell reads the script and reports syntax errors
      // without running a line of it.
      const proc = Bun.spawn([shell, "-n"], {
        stdin: new TextEncoder().encode(text),
        stdout: "pipe",
        stderr: "pipe",
      });
      const status = await proc.exited;
      const stderr = await new Response(proc.stderr).text();
      expect(stderr).toBe("");
      expect(status).toBe(0);
    });
  }

  test("at least one real shell graded the script, whatever this box carries", () => {
    // The guard on the skip above. Every PARSEABLE shell going missing would
    // leave the parse claim untested and this describe still green, which is
    // the shape of a gate that has stopped gating.
    expect(commandPresent(ALWAYS_PARSEABLE)).toBe(true);
    expect(PARSEABLE.filter(commandPresent).length).toBeGreaterThan(0);
  });

  test("a shell we do not offer is refused, naming the four we do", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["--completions", "powershell"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({}),
    });
    expect(code).not.toBe(0);
    const text = err.read();
    for (const shell of SHELLS) expect(text).toContain(shell);
  });
});

describe("the help advertises no flag this repository did not design", () => {
  test("--wizard is absent from the rendered help", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["--help"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({}),
    });
    expect(code).toBe(0);
    expect(out.read()).not.toContain("--wizard");
  });

  test("removing it from the help did not remove the rest of the block", async () => {
    const out = bufferStream();
    const err = bufferStream();
    await runCli(["--help"], { stdout: out.stream, stderr: err.stream, env: cliEnv({}) });
    const text = out.read();
    // The entry sat between --version and --completions; dropping a term line
    // plus its wrapped description is exactly the operation that eats a
    // neighbour if the indent rule is wrong.
    for (const flag of ["--help", "--version", "--completions", "--log-level"])
      expect(text).toContain(flag);
  });
});
