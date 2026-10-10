// A row in the by-identifier matrix that stopped validating client-side
// would send a malformed id to the API. Every row is driven through the
// command tree against a mock API, signed in, so the refusal is observed
// rather than read off the table.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { validateRequiredId } from "../src/lib/id.ts";
import { INVALID_ID_SAMPLES, REQUIRES_IDENTIFIER } from "./acceptance/fixtures/command-matrix.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { authedScope, withMockApi } from "./helpers-fleet-install.ts";

const JSON_FLAG = "--json";
const EXIT_OK = 0;

const refuseOffline = (argv: readonly string[]) =>
  authedScope(() =>
    withMockApi({}, async (apiUrl, calls) => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli([...argv, JSON_FLAG], {
        stdout: out.stream,
        stderr: err.stream,
        env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
      });
      return { code, output: `${out.read()}${err.read()}`, requests: calls.length };
    }),
  );

describe("every identifier-taking verb refuses a malformed id before any request", () => {
  const cases = REQUIRES_IDENTIFIER.flatMap((row) =>
    INVALID_ID_SAMPLES.map((sample) => ({ label: `${row.args.join(" ")} "${sample}"`, row, sample })),
  );

  test.each(cases)("$label is refused without a request", async ({ row, sample }) => {
    const result = await refuseOffline([...row.args, sample]);
    const refusal = validateRequiredId(sample, row.argName);
    if (refusal.ok) throw new Error(`sample ${sample} passes the id validator`);

    expect(result.code).not.toBe(EXIT_OK);
    expect(result.requests).toBe(0);
    expect(result.output).toContain(refusal.message);
  });
});

describe("a comment cites a file in a language this repository still has", () => {
  // A citation outlives its file silently. When a daemon is rewritten in
  // another language, every comment naming one of its source files keeps
  // sending readers to a file that no longer exists. So the extension of every
  // path a comment cites must be one git still tracks somewhere in the
  // repository: a citation into a retired language fails here. Only a path
  // with a directory is checked; a bare file name in prose cannot be told
  // apart from a domain name, so it is out of reach.
  const CITED_PATH = /(?<![\w./:-])(?:[\w-]+\/)+[\w.-]*\w(\.[a-z]{1,4})(?![\w/])/g;
  const COMMENT_LINE = /^\s*(?:\/\/|\/\*|\*)/;

  const retiredCitations = (text: string, tracked: ReadonlySet<string>): string[] =>
    text
      .split("\n")
      .filter((line) => COMMENT_LINE.test(line))
      .flatMap((line) => [...line.matchAll(CITED_PATH)])
      .filter(([, extension]) => !tracked.has(extension ?? ""))
      .map(([cited]) => cited);

  test("the check flags a cited extension nothing tracks, and passes one that is", () => {
    const tracked = new Set([".rs"]);
    expect(retiredCitations("// see fleet/sql.qq for the scan", tracked)).toEqual(["fleet/sql.qq"]);
    expect(retiredCitations("// see afd_fleet/src/lease.rs", tracked)).toEqual([]);
    expect(retiredCitations("const re = /not found/i.qq;", tracked)).toEqual([]);
  });

  test("no source or fixture comment cites a file in a retired language", async () => {
    const { readdirSync, readFileSync, statSync } = await import("node:fs");
    const { extname, join } = await import("node:path");
    const listed = Bun.spawnSync(["git", "ls-files"], { cwd: join(import.meta.dir, "..", "..") });
    expect(listed.exitCode).toBe(0);
    const tracked = new Set(listed.stdout.toString().split("\n").map((path) => extname(path)));
    const offenders: string[] = [];
    const walk = (dir: string): void => {
      for (const name of readdirSync(dir)) {
        const full = join(dir, name);
        if (statSync(full).isDirectory()) { walk(full); continue; }
        if (!name.endsWith(".ts")) continue;
        for (const cited of retiredCitations(readFileSync(full, "utf8"), tracked))
          offenders.push(`${full}: ${cited}`);
      }
    };
    walk(join(import.meta.dir, "..", "src"));
    walk(join(import.meta.dir, "..", "test"));
    expect(offenders).toEqual([]);
  });
});
