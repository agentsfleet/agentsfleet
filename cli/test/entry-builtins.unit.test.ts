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

// Shell completion is offered again because a generated script is now scoped
// to the credential that generated it; the wizard stays out.
test.each([{command: []}, {command: ["platform"]}])("omits the wizard and offers scoped completions in help for %j", async ({command}) => {
  const result = await run([...command, "--help"]);
  expect(result.code).toBe(0);
  expect(result.stdout).not.toContain("--wizard");
  expect(result.stdout).toContain("--completions");
  expect(result.stdout).toContain("--help");
  expect(result.stdout).toContain("--version");
});

test.each(["--wizard", "--completion"])("refuses removed flag %s", async flag => {
  const result = await run([flag]);
  expect(result.code).toBe(4);
  expect(result.stdout).toContain("USAGE");
  expect(result.stderr).toContain(flag);
  expect(result.stderr).toContain("Unrecognized flag");
});

test("returns a structured refusal for a removed flag on a nested command", async () => {
  const result = await run(["--json", "platform", "runners", "--wizard"]);
  expect(result.code).toBe(4);
  expect(result.stdout).toBe("");
  expect(JSON.parse(result.stderr)).toMatchObject({error: {code: "UNKNOWN_OPTION",
    message: "Unrecognized flag: --wizard in command agentsfleet platform runners"}});
});
