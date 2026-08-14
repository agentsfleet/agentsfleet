import { describe, test, expect } from "bun:test";
import os from "node:os";
import path from "node:path";

import { resolveConfigDir, STATE_DIR_ENV } from "../src/lib/config-dir.ts";

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
    // Invariant 3: neither state module reads the process environment at all.
    expect(state).not.toContain("process.env");
    const self = await read("../src/lib/config-dir.ts");
    expect(self).not.toContain("process.env");
  });
});
