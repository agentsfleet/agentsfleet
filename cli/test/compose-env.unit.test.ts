// Contract tests for composeEnv — the chokepoint that makes spawned-CLI
// acceptance tests hermetic. The acceptance fixture spawns the real CLI with
// a fully-composed child env (it never inherits process.env), so a regression
// here silently re-breaks the whole suite: dropping the telemetry-off default
// reintroduces the ~5s PostHog-flush flake; leaking parent env reintroduces
// token cross-contamination. These tests fail loudly when that happens.

import { describe, expect, test } from "bun:test";

import { composeEnv } from "./acceptance/fixtures/cli.js";

describe("composeEnv — hermetic spawn env", () => {
  test("should default AGENTSFLEET_TELEMETRY_DISABLED=1 so spawned CLIs never flush telemetry", () => {
    const env = composeEnv({ NO_COLOR: "1" });
    expect(env.AGENTSFLEET_TELEMETRY_DISABLED).toBe("1");
    expect(env.NO_COLOR).toBe("1");
  });

  test("should forward PATH and HOME from the parent so the spawned binary resolves", () => {
    const env = composeEnv({});
    expect(env.PATH).toBe(process.env.PATH);
    expect(env.HOME).toBe(process.env.HOME);
  });

  test("should let a caller override the telemetry default by listing the key in fields", () => {
    const env = composeEnv({ AGENTSFLEET_TELEMETRY_DISABLED: "0" });
    expect(env.AGENTSFLEET_TELEMETRY_DISABLED).toBe("0");
  });

  test("should omit null and undefined fields so a parent AGENTSFLEET_API_KEY never leaks into a spawn", () => {
    const env = composeEnv({ AGENTSFLEET_API_KEY: undefined, EXTRA: null, KEEP: "yes" });
    expect("AGENTSFLEET_API_KEY" in env).toBe(false);
    expect("EXTRA" in env).toBe(false);
    expect(env.KEEP).toBe("yes");
  });

  test("should coerce non-string field values to strings (spawn env requires strings)", () => {
    const env = composeEnv({ COUNT: 5, FLAG: true });
    expect(env.COUNT).toBe("5");
    expect(env.FLAG).toBe("true");
  });
});
