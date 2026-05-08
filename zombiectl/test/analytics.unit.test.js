import { test } from "bun:test";
import assert from "node:assert/strict";
import { cliAnalyticsInternals } from "../src/lib/analytics.js";

test("analytics resolveConfig disables telemetry by default with empty env", () => {
  const cfg = cliAnalyticsInternals.resolveConfig({});
  assert.equal(cfg.enabled, false);
  assert.equal(cfg.host, "https://us.i.posthog.com");
  assert.equal(cfg.key, cliAnalyticsInternals.DEFAULT_POSTHOG_KEY);
});

test("analytics resolveConfig opts in with DISABLE_TELEMETRY=0", () => {
  for (const value of ["0", "false", "off", "no"]) {
    const cfg = cliAnalyticsInternals.resolveConfig({ DISABLE_TELEMETRY: value });
    assert.equal(cfg.enabled, true, `expected enabled for DISABLE_TELEMETRY=${value}`);
    assert.equal(cfg.key, cliAnalyticsInternals.DEFAULT_POSTHOG_KEY);
  }
});

test("analytics resolveConfig stays off with DISABLE_TELEMETRY=1", () => {
  for (const value of ["1", "true", "on", "yes"]) {
    const cfg = cliAnalyticsInternals.resolveConfig({ DISABLE_TELEMETRY: value });
    assert.equal(cfg.enabled, false, `expected disabled for DISABLE_TELEMETRY=${value}`);
  }
});

test("analytics resolveConfig honors explicit env key override when opted in", () => {
  const cfg = cliAnalyticsInternals.resolveConfig({
    DISABLE_TELEMETRY: "0",
    ZOMBIE_POSTHOG_KEY: "phc_test",
  });
  assert.equal(cfg.enabled, true);
  assert.equal(cfg.key, "phc_test");
});

test("analytics resolveConfig ignores legacy ZOMBIE_POSTHOG_ENABLED env var", () => {
  // Pre-M63_006 the on-switch was ZOMBIE_POSTHOG_ENABLED. After the rename it
  // is dead — setting it neither enables nor disables anything.
  const cfg = cliAnalyticsInternals.resolveConfig({ ZOMBIE_POSTHOG_ENABLED: "true" });
  assert.equal(cfg.enabled, false);
});

test("analytics sanitizeProperties stringifies values and drops nullish", () => {
  const properties = cliAnalyticsInternals.sanitizeProperties({
    command: "run",
    exit_code: 0,
    empty: null,
    ignored: undefined,
    ok: false,
  });
  assert.deepEqual(properties, {
    command: "run",
    exit_code: "0",
    ok: "false",
  });
});
