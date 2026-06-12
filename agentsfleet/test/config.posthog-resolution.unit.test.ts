// CliConfig PostHog telemetry resolution. PostHog key/host moved from
// analytics.layer.ts into CliConfig (mirrors supabase
// next/config/cli-config.layer.ts) so the analytics layer reads
// resolved values via `yield* CliConfig` instead of dipping into
// process.env at capture time.

import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import {
  DEFAULT_POSTHOG_HOST,
  DEFAULT_POSTHOG_KEY,
  resolveCliConfig,
} from "../src/services/config.ts";

const ENV_KEYS = [
  "ZOMBIE_TELEMETRY_POSTHOG_KEY",
  "ZOMBIE_TELEMETRY_POSTHOG_HOST",
] as const;
const saved: Record<string, string | undefined> = {};

beforeEach(() => {
  for (const k of ENV_KEYS) saved[k] = process.env[k];
  for (const k of ENV_KEYS) delete process.env[k];
});

afterEach(() => {
  for (const k of ENV_KEYS) {
    if (saved[k] === undefined) delete process.env[k];
    else process.env[k] = saved[k];
  }
});

describe("CliConfig PostHog resolution", () => {
  it("exports DEFAULT_POSTHOG_HOST and DEFAULT_POSTHOG_KEY", () => {
    expect(DEFAULT_POSTHOG_HOST).toBe("https://us.i.posthog.com");
    expect(DEFAULT_POSTHOG_KEY.length).toBeGreaterThan(0);
    expect(DEFAULT_POSTHOG_KEY.startsWith("phc_")).toBe(true);
  });

  it("falls back to DEFAULT_POSTHOG_KEY/HOST when env is unset", () => {
    const cfg = resolveCliConfig();
    expect(cfg.telemetryPosthogKey).toBe(DEFAULT_POSTHOG_KEY);
    expect(cfg.telemetryPosthogHost).toBe(DEFAULT_POSTHOG_HOST);
  });

  it("ZOMBIE_TELEMETRY_POSTHOG_KEY env overrides default", () => {
    process.env.ZOMBIE_TELEMETRY_POSTHOG_KEY = "phc_test_override";
    const cfg = resolveCliConfig();
    expect(cfg.telemetryPosthogKey).toBe("phc_test_override");
  });

  it("ZOMBIE_TELEMETRY_POSTHOG_HOST env overrides default", () => {
    process.env.ZOMBIE_TELEMETRY_POSTHOG_HOST = "https://eu.i.posthog.com";
    const cfg = resolveCliConfig();
    expect(cfg.telemetryPosthogHost).toBe("https://eu.i.posthog.com");
  });

  it("falls back to defaults when env values are empty strings", () => {
    process.env.ZOMBIE_TELEMETRY_POSTHOG_KEY = "";
    process.env.ZOMBIE_TELEMETRY_POSTHOG_HOST = "";
    const cfg = resolveCliConfig();
    expect(cfg.telemetryPosthogKey).toBe(DEFAULT_POSTHOG_KEY);
    expect(cfg.telemetryPosthogHost).toBe(DEFAULT_POSTHOG_HOST);
  });

  it("falls back to defaults when env values are whitespace-only", () => {
    process.env.ZOMBIE_TELEMETRY_POSTHOG_KEY = "   ";
    process.env.ZOMBIE_TELEMETRY_POSTHOG_HOST = "\t\n";
    const cfg = resolveCliConfig();
    expect(cfg.telemetryPosthogKey).toBe(DEFAULT_POSTHOG_KEY);
    expect(cfg.telemetryPosthogHost).toBe(DEFAULT_POSTHOG_HOST);
  });
});
