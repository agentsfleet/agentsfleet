/**
 * The live preflight refuses a spawn environment that inherits the parent's
 * API key, and passes the composer every spec uses.
 */

import { afterEach, beforeEach, describe, expect, it } from "bun:test";

import { API_KEY_ENV } from "../../src/constants/env.ts";
import { assertHermeticSpawnEnv, type SpawnEnvComposer } from "./global-setup.ts";

const PARENT_KEY = "agt_t_parent_fixture_key";

const inheritingComposer: SpawnEnvComposer = (fields) => {
  const inherited: Record<string, string> = {};
  for (const [key, value] of Object.entries(process.env)) if (value !== undefined) inherited[key] = value;
  for (const [key, value] of Object.entries(fields)) if (value != null) inherited[key] = String(value);
  return inherited;
};

describe("the acceptance spawn-env preflight", () => {
  let saved: string | undefined;
  beforeEach(() => {
    saved = process.env[API_KEY_ENV];
    process.env[API_KEY_ENV] = PARENT_KEY;
  });
  afterEach(() => {
    if (saved === undefined) delete process.env[API_KEY_ENV];
    else process.env[API_KEY_ENV] = saved;
  });

  it("refuses a composer that lets a parent AGENTSFLEET_API_KEY through, without printing it", () => {
    expect(() => assertHermeticSpawnEnv(inheritingComposer)).toThrow(`${API_KEY_ENV} leaks into spawned CLIs`);
    try {
      assertHermeticSpawnEnv(inheritingComposer);
    } catch (error: unknown) {
      expect(String(error)).not.toContain(PARENT_KEY);
    }
  });

  it("passes the composer the acceptance specs spawn through", () => {
    expect(() => assertHermeticSpawnEnv()).not.toThrow();
  });
});
