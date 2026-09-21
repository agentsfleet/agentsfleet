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

describe("a comment cites a path that resolves", () => {
  // A citation outlives its file silently: the Zig error registry went with
  // the Zig daemon, and four comments kept pointing at it long enough to send
  // a reader looking for a file that had not existed for milestones.
  //
  // Scope: the paths below are the ones this workstream opened and corrected.
  // A wider sweep is owed — 16 further `.zig` citations remain across ten
  // files, listed in the spec's Out of Scope — and widening this array is
  // what will prove it done.
  const RETIRED_PATHS = ["error_registry.zig", "src/http/handlers/", "schema/embed.zig"] as const;

  test("no source or fixture comment points at a retired path", async () => {
    const { readdirSync, readFileSync, statSync } = await import("node:fs");
    const { join } = await import("node:path");
    const offenders: string[] = [];
    const walk = (dir: string): void => {
      for (const name of readdirSync(dir)) {
        const full = join(dir, name);
        if (statSync(full).isDirectory()) { walk(full); continue; }
        if (!name.endsWith(".ts")) continue;
        const text = readFileSync(full, "utf8");
        for (const retired of RETIRED_PATHS)
          if (text.includes(retired)) offenders.push(`${full}:${retired}`);
      }
    };
    walk(join(import.meta.dir, "..", "src"));
    walk(join(import.meta.dir, "..", "test"));
    // This file names the retired paths on purpose; it is the checker.
    expect(offenders.filter((o) => !o.includes("fixture-claims"))).toEqual([]);
  });
});
