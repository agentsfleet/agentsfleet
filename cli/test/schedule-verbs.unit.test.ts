// `schedule` spoke a private dialect: add where every other collection says
// create, rm where the rest says delete, and status where a single-resource
// read is called show — and that last one collided with the top-level status,
// which is a workspace read while this one takes two identifiers.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";

const RENAMED = [
  ["add", "create"],
  ["rm", "delete"],
  ["status", "show"],
] as const;

const help = async (argv: ReadonlyArray<string>) => {
  const out = bufferStream();
  const err = bufferStream();
  const code = await runCli([...argv], { stdout: out.stream, stderr: err.stream, env: cliEnv({}) });
  return { code, stdout: out.read(), stderr: err.read() };
};

describe("the schedule verbs match the rest of the surface", () => {
  test.each(RENAMED.map(([, to]) => to))("schedule %s is a real subcommand", async (verb) => {
    const { code, stdout } = await help(["schedule", verb, "--help"]);
    expect(code).toBe(0);
    expect(stdout).toContain(verb);
  });

  test.each(RENAMED.map(([from]) => from))(
    "the retired spelling %s is refused, pointing at the group's list",
    async (verb) => {
      const { code, stderr } = await help(["schedule", verb]);
      expect(code).not.toBe(0);
      expect(stderr).toContain(verb);
      expect(stderr).toContain("agentsfleet schedule --help");
    },
  );

  test("no compatibility alias survived either spelling", async () => {
    const { stdout } = await help(["schedule", "--help"]);
    for (const [from, to] of RENAMED) {
      expect(stdout).toContain(to);
      // The retired verb must be gone from the listing, not merely deprioritised.
      expect(stdout).not.toMatch(new RegExp(`^\\s+${from}\\s`, "m"));
    }
  });
});

describe("a refusal names a verb the parser accepts", () => {
  // The rename moved the verbs and left the usage lines behind, so a bad
  // identifier answered with `usage: agentsfleet schedule rm ...` — a sentence
  // naming the one spelling the parser had just stopped accepting. A
  // suggestion that cannot be typed is worse than none: it costs the reader a
  // second failure to find out.
  const BAD_ID = "not-a-uuid";

  test.each([
    ["create", ["schedule", "create", BAD_ID, "--cron", "* * * * *", "--message", "hi"]],
    ["list", ["schedule", "list", BAD_ID]],
    ["update", ["schedule", "update", BAD_ID, BAD_ID, "--message", "hi"]],
    ["delete", ["schedule", "delete", BAD_ID, BAD_ID]],
    ["sync", ["schedule", "sync", BAD_ID, BAD_ID]],
    ["show", ["schedule", "show", BAD_ID, BAD_ID]],
  ])("%s refuses a malformed id naming its own spelling", async (verb, argv) => {
    const { code, stderr } = await help(argv as ReadonlyArray<string>);
    expect(code).not.toBe(0);
    expect(stderr).toContain(`agentsfleet schedule ${verb}`);
    for (const [retired] of RENAMED)
      expect(stderr).not.toContain(`agentsfleet schedule ${retired}`);
  });
});

describe("no command description names a scheduling vendor", () => {
  // The schedule surface named the host it happened to run on. Which host
  // receives a re-applied schedule is not a caller's concern, and naming it
  // dates the help the moment the platform moves.
  const VENDORS = ["qstash", "upstash", "cloudflare", "vercel cron"] as const;

  test("schedule --help names no scheduling vendor", async () => {
    const { code, stdout } = await help(["schedule", "--help"]);
    expect(code).toBe(0);
    const listing = stdout.toLowerCase();
    expect(VENDORS.filter((vendor) => listing.includes(vendor))).toEqual([]);
  });
});
