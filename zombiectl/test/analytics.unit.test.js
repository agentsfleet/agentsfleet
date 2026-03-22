import { test } from "bun:test";
import assert from "node:assert/strict";
import { cliAnalyticsInternals } from "../src/lib/analytics.js";

test("analytics resolveConfig enables bundled default key when env key is absent", () => {
  const cfg = cliAnalyticsInternals.resolveConfig({});
  assert.equal(cfg.enabled, true);
  assert.equal(cfg.host, "https://us.i.posthog.com");
  assert.equal(cfg.key, cliAnalyticsInternals.DEFAULT_POSTHOG_KEY);
});

test("analytics resolveConfig honors explicit env key override", () => {
  const cfg = cliAnalyticsInternals.resolveConfig({
    ZOMBIE_POSTHOG_KEY: "phc_test",
  });
  assert.equal(cfg.enabled, true);
  assert.equal(cfg.key, "phc_test");
});

test("analytics resolveConfig allows opt-out even with bundled key", () => {
  const cfg = cliAnalyticsInternals.resolveConfig({
    ZOMBIE_POSTHOG_ENABLED: "false",
  });
  assert.equal(cfg.enabled, false);
  assert.equal(cfg.key, cliAnalyticsInternals.DEFAULT_POSTHOG_KEY);
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
