import { test } from "bun:test";
import assert from "node:assert/strict";
import { cliAnalyticsInternals } from "../src/lib/analytics.js";

test("analytics resolveConfig defaults to disabled when env and preferences absent", () => {
  const cfg = cliAnalyticsInternals.resolveConfig({});
  assert.equal(cfg.enabled, false);
  assert.equal(cfg.host, "https://us.i.posthog.com");
  assert.equal(cfg.key, cliAnalyticsInternals.DEFAULT_POSTHOG_KEY);
});

test("analytics resolveConfig disabled even when bundled key present and consent is null", () => {
  const cfg = cliAnalyticsInternals.resolveConfig({}, { posthog_enabled: null });
  assert.equal(cfg.enabled, false);
  assert.equal(cfg.key, cliAnalyticsInternals.DEFAULT_POSTHOG_KEY);
});

test("analytics resolveConfig honors explicit env key override but stays disabled without consent", () => {
  const cfg = cliAnalyticsInternals.resolveConfig({
    ZOMBIE_POSTHOG_KEY: "phc_test",
  });
  assert.equal(cfg.enabled, false);
  assert.equal(cfg.key, "phc_test");
});

test("analytics resolveConfig env true beats preferences false", () => {
  const cfg = cliAnalyticsInternals.resolveConfig(
    { ZOMBIE_POSTHOG_ENABLED: "true" },
    { posthog_enabled: false },
  );
  assert.equal(cfg.enabled, true);
});

test("analytics resolveConfig env false beats preferences true", () => {
  const cfg = cliAnalyticsInternals.resolveConfig(
    { ZOMBIE_POSTHOG_ENABLED: "false" },
    { posthog_enabled: true },
  );
  assert.equal(cfg.enabled, false);
});

test("analytics resolveConfig empty env string falls through to preferences", () => {
  const cfg = cliAnalyticsInternals.resolveConfig(
    { ZOMBIE_POSTHOG_ENABLED: "" },
    { posthog_enabled: true },
  );
  assert.equal(cfg.enabled, true);
});

test("analytics resolveConfig preferences true wins when env absent", () => {
  const cfg = cliAnalyticsInternals.resolveConfig({}, { posthog_enabled: true });
  assert.equal(cfg.enabled, true);
});

test("analytics resolveConfig preferences false wins when env absent", () => {
  const cfg = cliAnalyticsInternals.resolveConfig({}, { posthog_enabled: false });
  assert.equal(cfg.enabled, false);
});

test("analytics resolveConfig allows opt-out via env even with bundled key", () => {
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
