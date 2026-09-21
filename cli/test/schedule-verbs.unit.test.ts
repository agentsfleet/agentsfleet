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

describe("no command description names a scheduling vendor", () => {
  // The schedule surface named the host it happened to run on. Which host
  // receives a re-applied schedule is not a caller's concern, and naming it
  // dates the help the moment the platform moves.
  const VENDORS = ["qstash", "upstash", "cloudflare", "vercel cron"] as const;

  test("the command tree's descriptions are vendor-free", async () => {
    const { readdirSync, readFileSync } = await import("node:fs");
    const { join } = await import("node:path");
    const dir = join(import.meta.dir, "..", "src", "program", "tree");
    const offenders: string[] = [];
    for (const file of readdirSync(dir).filter((f) => f.endsWith(".ts"))) {
      const text = readFileSync(join(dir, file), "utf8").toLowerCase();
      for (const vendor of VENDORS) if (text.includes(vendor)) offenders.push(`${file}:${vendor}`);
    }
    expect(offenders).toEqual([]);
  });

  test("sync says what it does, not where it sends it", async () => {
    const { stdout } = await help(["schedule", "--help"]);
    expect(stdout).toContain("Re-apply a hosted schedule");
  });
});
