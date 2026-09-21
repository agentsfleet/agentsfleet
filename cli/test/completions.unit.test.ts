// The two global flags this CLI advertises but never covered.
//
// `--completions` emitted thousands of lines that nothing ever parsed, so a
// change to the command tree could have shipped a script the shell rejects
// and no test would have noticed. `--wizard` is the opposite problem: it
// works, this repository never designed it, and the help offered it anyway.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";

const SHELLS = ["bash", "zsh", "fish", "sh"] as const;
// The two we can hand back to their own parser on this machine.
const PARSEABLE = ["bash", "zsh"] as const;

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

  test.each([...PARSEABLE])("%s accepts what we emit for it", async (shell) => {
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
