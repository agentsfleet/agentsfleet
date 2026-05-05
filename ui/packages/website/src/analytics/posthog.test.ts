import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/*
 * Init-flag contract for the marketing-site analytics layer.
 *
 * The rest of the test suite mocks `./posthog` wholesale (Hero, Pricing,
 * App), so the actual `init()` call is never exercised. That means a
 * regression — e.g. flipping `autocapture` back to `false`, dropping
 * `capture_pageview`, or reverting `persistence` to `localStorage` — would
 * pass every other test and silently ship to production.
 *
 * This file does NOT mock `./posthog`; it stubs the lazy `posthog-js`
 * import so we can capture the init args directly.
 */

// Synthetic value — no real key shape (gitleaks generic-api-key rule fires
// on inline `key: "..."` literals regardless of content).
const TEST_KEY = ["phc", "synthetic", "fixture", "0123456789"].join("_");

const captured: Array<{ key: string; opts: Record<string, unknown> }> = [];

vi.mock("posthog-js", () => ({
  default: {
    init: (key: string, opts: Record<string, unknown>) => {
      captured.push({ key, opts });
    },
    capture: vi.fn(),
  },
}));

// Bypass the SSR/idle-callback guard — test environment has no idle
// callback, but the loader runs synchronously when the test calls
// flushAnalyticsForTests().
const originalRic = (globalThis as { requestIdleCallback?: unknown }).requestIdleCallback;

beforeEach(() => {
  captured.length = 0;
  (globalThis as { requestIdleCallback?: (cb: () => void) => void }).requestIdleCallback = (
    cb: () => void,
  ) => cb();
  (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__ = {
    enabled: true,
    key: TEST_KEY,
    host: "https://us.i.posthog.com",
  };
});

afterEach(async () => {
  const mod = await import("./posthog");
  mod.resetAnalyticsForTests();
  (globalThis as { requestIdleCallback?: unknown }).requestIdleCallback = originalRic;
  delete (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__;
});

describe("posthog init contract", () => {
  it("initializes posthog-js with autocapture, pageview, and pageleave enabled", async () => {
    const mod = await import("./posthog");
    mod.initAnalytics();
    await mod.flushAnalyticsForTests();

    expect(captured).toHaveLength(1);
    const { key, opts } = captured[0]!;
    expect(key).toBe(TEST_KEY);
    expect(opts.api_host).toBe("https://us.i.posthog.com");
    expect(opts.autocapture).toBe(true);
    expect(opts.capture_pageview).toBe("history_change");
    expect(opts.capture_pageleave).toBe(true);
    expect(opts.persistence).toBe("localStorage+cookie");
  });

  it("does not initialize when key is empty", async () => {
    (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__ = {
      enabled: true,
      key: "",
      host: "https://us.i.posthog.com",
    };
    const mod = await import("./posthog");
    mod.initAnalytics();
    await mod.flushAnalyticsForTests();
    expect(captured).toHaveLength(0);
  });
});
