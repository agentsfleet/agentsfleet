import type React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import posthog from "posthog-js";
import CTABlock from "../components/CTABlock";
import {
  EVENT_NAVIGATION_CLICKED,
  EVENT_SIGNUP_STARTED,
  flushAnalyticsForTests,
  initAnalytics,
  resetAnalyticsForTests,
  trackNavigationClicked,
  trackSignupStarted,
} from "./posthog";

function walkElements(node: React.ReactNode, visit: (element: React.ReactElement<Record<string, unknown>>) => void): void {
  if (!node || typeof node !== "object") return;
  const element = node as React.ReactElement<Record<string, unknown>>;
  visit(element);
  const children = element.props?.children;
  if (Array.isArray(children)) {
    children.forEach((child) => walkElements(child, visit));
    return;
  }
  walkElements(children as React.ReactNode, visit);
}

function ctaOnClick(label: string) {
  const tree = CTABlock();
  let handler: (() => void) | undefined;
  walkElements(tree, (element) => {
    if (handler) return;
    if (typeof element.props?.onClick !== "function") return;
    if (typeof element.props?.children !== "string") return;
    if (element.props.children !== label) return;
    handler = element.props.onClick as () => void;
  });
  return handler;
}

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

  it("captures agent-safe CTA navigation events", async () => {
    ctaOnClick("→ read quickstart")?.();
    ctaOnClick("view pricing")?.();
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

  it("does not emit when analytics is disabled", async () => {
    resetAnalyticsForTests();
    (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__ = {
      enabled: false,
      key: "phc_test_key",
      host: "https://us.i.posthog.com",
    };

    ctaOnClick("→ read quickstart")?.();
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
});
