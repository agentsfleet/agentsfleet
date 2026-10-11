// composeEnv builds the whole environment of every CLI the acceptance specs
// spawn. Its telemetry-off default is what keeps a spawned CLI from waiting
// ~5s on a PostHog flush when the runner is offline; the CLI's own consent
// resolver decides here whether that default still turns telemetry off.

import { rmSync } from "node:fs";
import { afterEach, describe, expect, test } from "bun:test";

import { STATE_DIR_ENV } from "../src/constants/env.ts";
import { getEffectiveConsent } from "../src/services/telemetry/consent.ts";
import { composeEnv } from "./acceptance/fixtures/cli.js";

// Read only as a dotted property in src/, so src/constants/env.ts has no
// constant for it (see the note at the top of that file).
const TELEMETRY_DISABLED_ENV = "AGENTSFLEET_TELEMETRY_DISABLED";
const TELEMETRY_ENABLED = "0";

const stateDirs: string[] = [];

function spawnEnv(fields: Parameters<typeof composeEnv>[0]): NodeJS.ProcessEnv {
  const env = composeEnv(fields);
  const stateDir = env[STATE_DIR_ENV];
  if (stateDir) stateDirs.push(stateDir);
  return env;
}

afterEach(() => {
  for (const dir of stateDirs.splice(0)) rmSync(dir, { recursive: true, force: true });
});

describe("composeEnv's telemetry default", () => {
  test("a spawned CLI resolves telemetry consent as denied when the caller says nothing", () => {
    expect(getEffectiveConsent(null, spawnEnv({}))).toBe("denied");
  });

  test("a caller that lists the telemetry key gets its own value, and consent follows it", () => {
    const env = spawnEnv({ [TELEMETRY_DISABLED_ENV]: TELEMETRY_ENABLED });
    expect(env[TELEMETRY_DISABLED_ENV]).toBe(TELEMETRY_ENABLED);
    expect(getEffectiveConsent(null, env)).toBe("granted");
  });
});
