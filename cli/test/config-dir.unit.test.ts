import { describe, test, expect } from "bun:test";
import os from "node:os";
import path from "node:path";

import { resolveConfigDir } from "../src/lib/config-dir.ts";
import { STATE_DIR_ENV } from "../src/constants/env.ts";
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
