import { describe, test, expect } from "bun:test";
import os from "node:os";
import path from "node:path";

import { resolveConfigDir, STATE_DIR_ENV } from "../src/lib/config-dir.ts";
import { cliEnv } from "./helpers-cli-state.ts";

describe("resolveConfigDir", () => {
  test("honours the supplied environment", () => {
    expect(resolveConfigDir({ [STATE_DIR_ENV]: "/x" })).toBe("/x");
  });

  test("falls back to the home default when unset or empty", () => {
    const home = path.join(os.homedir(), ".config", "agentsfleet");
    expect(resolveConfigDir({})).toBe(home);
    expect(resolveConfigDir({ [STATE_DIR_ENV]: "" })).toBe(home);
  });

  test("the resolution has one declaration site — neither prior copy survives", async () => {
    const read = (rel: string) => Bun.file(new URL(rel, import.meta.url)).text();
    const state = await read("../src/lib/state.ts");
    const consent = await read("../src/services/telemetry/consent.ts");
    for (const source of [state, consent]) {
      expect(source).not.toContain(STATE_DIR_ENV);
      expect(source).not.toContain('".config"');
      expect(source).not.toContain("process.env." + STATE_DIR_ENV);
    }
    // Neither state module reads the process environment at all.
    expect(state).not.toContain("process.env");
    const self = await read("../src/lib/config-dir.ts");
    expect(self).not.toContain("process.env");
  });

  test("no file under src/ names the state-dir variable except its declaration site", async () => {
    // Suite-level, not review-level: a re-introduced literal anywhere in
    // src/ — a new service, a command, a helper — fails here rather than
    // depending on a reviewer to run the grep. The declaration site is the
    // one file allowed to spell it.
    const srcRoot = new URL("../src/", import.meta.url).pathname;
    const glob = new Bun.Glob("**/*.ts");
    const declarationSite = path.join("lib", "config-dir.ts");
    const candidates: string[] = [];
    for await (const rel of glob.scan(srcRoot)) {
      if (rel !== declarationSite) candidates.push(rel);
    }
    // Read together rather than one at a time — this walks all of src/.
    const bodies = await Promise.all(
      candidates.map((rel) => Bun.file(path.join(srcRoot, rel)).text()),
    );
    const offenders = candidates.filter((_, i) => bodies[i]?.includes(STATE_DIR_ENV));
    expect(offenders).toEqual([]);
  });
});

describe("cliEnv", () => {
  test("refuses to build an env with no state dir, rather than escaping the sandbox", () => {
    // This throw is the single funnel protecting ~29 test files from resolving
    // the store to the operator's real ~/.config/agentsfleet. test/** is
    // excluded from the coverage floor, so nothing else would notice if a
    // refactor defaulted stateDirEnv() and the net went dead.
    const previous = process.env[STATE_DIR_ENV];
    delete process.env[STATE_DIR_ENV];
    try {
      expect(() => cliEnv()).toThrow(STATE_DIR_ENV);
      expect(() => cliEnv({ AGENTSFLEET_API_URL: "https://x" })).toThrow(/unset/);
    } finally {
      if (previous === undefined) delete process.env[STATE_DIR_ENV];
      else process.env[STATE_DIR_ENV] = previous;
    }
  });
});
