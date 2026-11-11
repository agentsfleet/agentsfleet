import { expect, test } from "bun:test";
import { runCli } from "../src/cli.ts";

async function run(args: ReadonlyArray<string>) {
  let stdout = "";
  let stderr = "";
  const code = await runCli(args, {
    stdout: {write: (chunk: string) => { stdout += chunk; return true; }, isTTY: false},
    stderr: {write: (chunk: string) => { stderr += chunk; return true; }, isTTY: false},
    env: {NO_COLOR: "1"},
  });
  return {code, stdout, stderr};
}

test.each([{command: []}, {command: ["platform"]}])("omits wizard and shell generation from help for %j", async ({command}) => {
  const result = await run([...command, "--help"]);
  expect(result.code).toBe(0);
  expect(result.stdout).not.toContain("--wizard");
  expect(result.stdout).not.toContain("--completions");
  expect(result.stdout).toContain("--help");
  expect(result.stdout).toContain("--version");
});

test.each(["--wizard", "--completions", "--completion"])("refuses removed flag %s", async flag => {
  const result = await run([flag, ...(flag === "--completions" ? ["bash"] : [])]);
  expect(result.code).toBe(4);
  expect(result.stdout).toContain("USAGE");
  expect(result.stderr).toContain(flag);
  expect(result.stderr).toContain("Unrecognized flag");
});

test("returns a structured refusal for a removed flag on a nested command", async () => {
  const result = await run(["--json", "platform", "runners", "--completions", "bash"]);
  expect(result.code).toBe(4);
  expect(result.stdout).toBe("");
  expect(JSON.parse(result.stderr)).toMatchObject({error: {code: "UNKNOWN_OPTION",
    message: "Unrecognized flag: --completions in command agentsfleet platform runners"}});
});
