import test from "node:test";
import assert from "node:assert/strict";
import { cliAnalyticsInternals } from "../src/lib/analytics.js";

test("analytics resolveConfig enables when key exists", () => {
  const cfg = cliAnalyticsInternals.resolveConfig({
    ZOMBIE_POSTHOG_KEY: "phc_test",
  });
  assert.equal(cfg.enabled, true);
  assert.equal(cfg.host, "https://us.i.posthog.com");
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
