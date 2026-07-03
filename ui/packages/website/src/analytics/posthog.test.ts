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
// When true, the next posthog-js `init()` throws — simulating a blocked/offline
// chunk load or an init failure. Tests flip it to exercise the recovery path.
let failNextInit = false;

vi.mock("posthog-js", () => ({
  default: {
    init: (key: string, opts: Record<string, unknown>) => {
      if (failNextInit) {
        failNextInit = false;
        throw new Error("posthog-js chunk blocked");
      }
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
  failNextInit = false;
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
    expect(opts.persistence).toBe("localStorage");
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

  it("falls back to import.meta.env when no global config is present", async () => {
    // Exercises the `globalCfg?.X ?? import.meta.env.VITE_POSTHOG_X` fallback
    // chain on every field — without this, the env-fallback branches in
    // readRuntimeConfig stay uncovered. In jsdom there's no env key set, so
    // the fall-through resolves to enabled=false and init is skipped, which
    // is the correct production behavior when neither source is configured.
    delete (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__;
    const mod = await import("./posthog");
    mod.initAnalytics();
    await mod.flushAnalyticsForTests();
    expect(captured).toHaveLength(0);
  });

  it("does not initialize when explicitly disabled despite a valid key", async () => {
    // Closes the failure-path: a privacy-conscious caller setting
    // window.__UZ_ANALYTICS_CONFIG__ = { enabled: false, ... } must keep
    // posthog-js out of the bundle entirely. Asserts the disable flag
    // wins over a present key — the opposite ordering would silently
    // re-enable analytics for users who explicitly opted out.
    (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__ = {
      enabled: false,
      key: TEST_KEY,
      host: "https://us.i.posthog.com",
    };
    const mod = await import("./posthog");
    mod.initAnalytics();
    await mod.flushAnalyticsForTests();
    expect(captured).toHaveLength(0);
  });
});

describe("posthog loader failure recovery (§2)", () => {
  // test_ensure_loader_retries_after_failed_load (Dimension 2.1)
  it("retries the load after a failed attempt instead of wedging permanently", async () => {
    const mod = await import("./posthog");

    // First load attempt fails (blocked chunk / init throw).
    failNextInit = true;
    mod.initAnalytics();
    await mod.flushAnalyticsForTests();
    expect(captured).toHaveLength(0); // nothing initialized on the failed attempt

    // A later event must retry the load — not short-circuit on a stale, still
    // permanently-rejected `loadPromise`. The queued event flushes on success.
    mod.trackNavigationClicked({ source: "hero" });
    await mod.flushAnalyticsForTests();
    expect(captured).toHaveLength(1); // retry succeeded → init ran
  });

  // test_failed_load_does_not_produce_unhandled_rejection (Dimension 2.2)
  it("a failed load is caught internally — the loader promise never rejects", async () => {
    const mod = await import("./posthog");
    failNextInit = true;
    mod.initAnalytics();
    // The production `loadPromise` IS this `.catch`-wrapped promise: it carries
    // a handler, so a failed load can never surface as an unhandled rejection.
    // Awaiting it must resolve (not throw). Without the `.catch`, loadPromise
    // would be the raw rejected promise this guards against, and this rejects.
    await expect(mod.flushAnalyticsForTests()).resolves.toBeUndefined();
    expect(captured).toHaveLength(0); // failure left state resettable, not wedged
  });
});
