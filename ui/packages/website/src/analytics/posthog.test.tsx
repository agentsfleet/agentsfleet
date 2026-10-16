import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import posthog from "posthog-js";
import {
  EVENT_NAVIGATION_CLICKED,
  EVENT_SIGNUP_STARTED,
  flushAnalyticsForTests,
  initAnalytics,
  resetAnalyticsForTests,
  trackNavigationClicked,
  trackSignupStarted,
  trackInteractionClicked,
} from "./posthog";

vi.mock("posthog-js", () => ({
  default: {
    init: vi.fn(),
    capture: vi.fn(),
  },
}));

const mockedPosthog = vi.mocked(posthog, true);

describe("website analytics", () => {
  beforeEach(() => {
    resetAnalyticsForTests();
    mockedPosthog.init.mockReset();
    mockedPosthog.capture.mockReset();
    (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__ = {
      enabled: true,
      key: "phc_test_key",
      host: "https://us.i.posthog.com",
    };
  });

  afterEach(() => {
    delete (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__;
  });

  it("emits signup_started with privacy-safe allowlisted properties", async () => {
    trackSignupStarted({
      source: "hero_primary",
      surface: "hero",
      mode: "humans",
      // oxlint-disable-next-line typescript/no-explicit-any
      email: "should-not-leak@example.com" as any,
    });
    await flushAnalyticsForTests();

    expect(mockedPosthog.init).toHaveBeenCalledTimes(1);
    expect(mockedPosthog.init).toHaveBeenCalledWith("phc_test_key", expect.objectContaining({
      advanced_disable_flags: true, autocapture: true, capture_pageview: "history_change", capture_pageleave: true,
    }));
    expect(mockedPosthog.capture).toHaveBeenCalledTimes(1);
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      EVENT_SIGNUP_STARTED,
      expect.objectContaining({
        source: "hero_primary",
        surface: "hero",
        mode: "humans",
      }),
    );
    const props = mockedPosthog.capture.mock.calls[0]?.[1] as Record<string, unknown>;
    expect(props.email).toBeUndefined();
    expect(props.path).toBeDefined();
  });

  it("captures Fleet-safe CTA navigation events", async () => {
    trackNavigationClicked({ source: "agents_cta_docs", surface: "cta_block" });
    trackNavigationClicked({ source: "agents_cta_pricing", surface: "cta_block" });
    await flushAnalyticsForTests();

    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      EVENT_NAVIGATION_CLICKED,
      expect.objectContaining({ source: "agents_cta_docs" }),
    );
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      EVENT_NAVIGATION_CLICKED,
      expect.objectContaining({ source: "agents_cta_pricing" }),
    );
  });

  it.each([EVENT_NAVIGATION_CLICKED, "$pageview", "$autocapture"])("filters enriched %s payloads before transport", async (event) => {
    trackNavigationClicked({ source: "header_nav_docs", surface: "header" });
    await flushAnalyticsForTests();
    const hook = mockedPosthog.init.mock.calls[0]?.[1]?.before_send;
    expect(hook).toBeTypeOf("function");
    if (typeof hook !== "function") throw new Error("Missing outgoing privacy filter");
    window.history.replaceState({}, "", "/agents?private=query#private-fragment");
    try {
      const identity = { token: "synthetic-token", distinct_id: "visitor", $session_id: "session", $device_id: "device" };
      const filtered = hook({
        uuid: "synthetic-event", event,
        properties: {
          ...identity, source: "header_nav_docs", surface: "header", path: "/?private=query#fragment",
          $current_url: window.location.href, $referrer: "https://example.test/?private=referrer#fragment",
          $session_entry_url: window.location.href, utm_source: "private-campaign",
          $session_entry_utm_source: "private-campaign", $elements_chain: "private-element-data",
          $set: { $current_url: window.location.href }, $set_once: { $initial_current_url: window.location.href },
        },
        $set: { $current_url: window.location.href }, $set_once: { $initial_referrer: "private-referrer" },
      });
      expect(filtered).toEqual({ uuid: "synthetic-event", event, properties: { ...identity, source: "header_nav_docs", surface: "header", path: "/agents" } });
      expect(hook(null)).toBeNull();
    } finally {
      window.history.replaceState({}, "", "/");
    }
  });

  it("does not emit when analytics is disabled", async () => {
    resetAnalyticsForTests();
    (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__ = {
      enabled: false,
      key: "phc_test_key",
      host: "https://us.i.posthog.com",
    };

    trackNavigationClicked({ source: "agents_cta_docs", surface: "cta_block" });
    await flushAnalyticsForTests();

    expect(mockedPosthog.init).not.toHaveBeenCalled();
    expect(mockedPosthog.capture).not.toHaveBeenCalled();
  });

  it("treats enabled=true with empty key as disabled", async () => {
    resetAnalyticsForTests();
    (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__ = {
      enabled: true,
      key: "",
      host: "https://us.i.posthog.com",
    };

    trackSignupStarted({ source: "hero_primary", surface: "hero", mode: "humans" });
    await flushAnalyticsForTests();

    expect(mockedPosthog.init).not.toHaveBeenCalled();
    expect(mockedPosthog.capture).not.toHaveBeenCalled();
  });

  it("truncates string properties longer than 256 characters", async () => {
    const longSource = "s".repeat(300);
    trackSignupStarted({ source: longSource, surface: "hero", mode: "humans" });
    await flushAnalyticsForTests();

    expect(mockedPosthog.capture).toHaveBeenCalledTimes(1);
    const props = mockedPosthog.capture.mock.calls[0]?.[1] as Record<string, unknown>;
    expect(typeof props.source).toBe("string");
    expect((props.source as string).length).toBe(256);
    expect(props.surface).toBe("hero");
  });

  it("is idempotent across repeated initAnalytics() calls", async () => {
    initAnalytics();
    initAnalytics();
    initAnalytics();
    // Force the lazy loader to resolve by emitting one event, then flush.
    trackSignupStarted({ source: "hero_primary", surface: "hero", mode: "humans" });
    await flushAnalyticsForTests();

    // Three init() calls → still exactly one underlying posthog.init.
    expect(mockedPosthog.init).toHaveBeenCalledTimes(1);
  });

  it("uses requestIdleCallback when available to defer module load", async () => {
    resetAnalyticsForTests();
    const ric = vi.fn((fn: () => void) => {
      fn();
      return 1;
    });
    (globalThis as { requestIdleCallback?: unknown }).requestIdleCallback = ric;
    try {
      initAnalytics();
      await flushAnalyticsForTests();
      expect(ric).toHaveBeenCalledTimes(1);
      expect(mockedPosthog.init).toHaveBeenCalledTimes(1);
    } finally {
      delete (globalThis as { requestIdleCallback?: unknown }).requestIdleCallback;
    }
  });

  it("captures synchronously once the posthog module has loaded", async () => {
    // First event primes the lazy loader.
    trackSignupStarted({ source: "hero_primary", surface: "hero", mode: "humans" });
    await flushAnalyticsForTests();
    expect(mockedPosthog.capture).toHaveBeenCalledTimes(1);

    // Second event arrives AFTER the module is resolved — should hit the
    // fast path (direct capture, no buffering).
    trackNavigationClicked({ source: "agents_cta_pricing", surface: "cta_block" });
    // No flush needed — fast path is synchronous.
    expect(mockedPosthog.capture).toHaveBeenCalledTimes(2);
    expect(mockedPosthog.capture).toHaveBeenLastCalledWith(
      EVENT_NAVIGATION_CLICKED,
      expect.objectContaining({ source: "agents_cta_pricing" }),
    );
  });

  it("keeps draining buffered clicks when the analytics client rejects one", async () => {
    mockedPosthog.capture.mockImplementationOnce(() => { throw new Error("blocked capture"); });
    trackInteractionClicked({ source: "cli_install_copy" });
    trackNavigationClicked({ source: "cli_install_guide" });
    await expect(flushAnalyticsForTests()).resolves.toBeUndefined();
    expect(mockedPosthog.capture).toHaveBeenCalledTimes(2);
    expect(mockedPosthog.capture).toHaveBeenLastCalledWith(EVENT_NAVIGATION_CLICKED, expect.objectContaining({ source: "cli_install_guide" }));
  });

  it("contains a loaded client's failure so the caller can finish its action", async () => {
    initAnalytics();
    trackNavigationClicked({ source: "prime" });
    await flushAnalyticsForTests();
    mockedPosthog.capture.mockImplementationOnce(() => { throw new Error("blocked capture"); });
    expect(() => trackInteractionClicked({ source: "cli_install_copy" })).not.toThrow();
    trackNavigationClicked({ source: "cli_install_guide" });
    expect(mockedPosthog.capture).toHaveBeenLastCalledWith(EVENT_NAVIGATION_CLICKED, expect.objectContaining({ source: "cli_install_guide" }));
  });
});
