// The acceptance fixtures described a looser CLI than this repository ships,
// and the cost was not cosmetic: a row marked as not validating client-side is
// EXCLUDED from the sweep that proves no network call fires, so real
// validation went unasserted on the strength of a comment.

import { describe, test, expect } from "bun:test";

import { REQUIRES_IDENTIFIER } from "./acceptance/fixtures/command-matrix.ts";

describe("the matrix describes the CLI we ship", () => {
  test("every identifier-taking verb is marked as validating client-side", () => {
    // Measured against an unroutable API: each of these answers
    // INVALID_ARGUMENT with the uuidv7 example and issues no request.
    const notValidating = REQUIRES_IDENTIFIER
      .filter((row) => !row.validatesClient)
      .map((row) => row.args.join(" "));
    expect(notValidating).toEqual([]);
  });

  test("the sweep that proves no dial therefore covers every row", () => {
    const swept = REQUIRES_IDENTIFIER.filter((row) => row.validatesClient);
    expect(swept.length).toBe(REQUIRES_IDENTIFIER.length);
    expect(swept.length).toBeGreaterThanOrEqual(8);
  });

  test("status is not in the by-identifier matrix — it takes no positional", () => {
    const byStatus = REQUIRES_IDENTIFIER.filter(
      (row) => row.args.length === 1 && row.args[0] === "status",
    );
    expect(byStatus).toEqual([]);
    // The one-fleet read is its own verb, and it IS a by-identifier probe.
    expect(REQUIRES_IDENTIFIER.some((row) => row.args.join(" ") === "fleet show")).toBe(true);
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
