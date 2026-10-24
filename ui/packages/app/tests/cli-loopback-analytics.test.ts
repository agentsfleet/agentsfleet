import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { filterAuthenticationEvent, isSensitiveAuthUrl } from "../lib/analytics/auth-privacy";
import { EVENTS } from "../lib/analytics/events";

const client = vi.hoisted(() => ({ init: vi.fn(), capture: vi.fn(), identify: vi.fn(), group: vi.fn(), setPersonProperties: vi.fn(), reset: vi.fn() }));
const ORIGIN = "https://app.example.test";

beforeEach(() => {
  vi.resetModules();
  vi.clearAllMocks();
  vi.doMock("posthog-js", () => ({ default: client }));
  vi.stubEnv("NEXT_PUBLIC_POSTHOG_KEY", "test-public-project-key");
  vi.stubEnv("NEXT_PUBLIC_POSTHOG_ENABLED", "true");
  vi.stubGlobal("requestIdleCallback", (callback: () => void) => { callback(); return 1; });
  window.history.replaceState(null, "", "/workspaces");
  window.localStorage.clear();
});

describe("queued telemetry ownership before the first SDK load", () => {
  it.each([
    { name: "account A", identity: { id: "account-A" } },
    { name: "unresolved identity", identity: undefined },
    { name: "signed-out identity", identity: null },
  ])("discards $name telemetry when the committed reader changes to B during idle startup", async ({ identity }) => {
    let resume!: () => void;
    vi.stubGlobal("requestIdleCallback", (callback: () => void) => { resume = callback; return 1; });
    const analytics = await import("../lib/analytics/posthog");
    const startup = analytics.initAnalytics(() => identity);
    analytics.setAnalyticsContext({ workspaceId: "A-workspace", workspaceCount: 7, plan: "A-plan" });
    analytics.captureProductEvent(EVENTS.fleet_viewed, { fleet_id: "A-fleet", status: "running" });
    const replacement = analytics.initAnalytics(() => ({ id: "account-B" }));
    resume(); await Promise.all([startup, replacement]);
    expect(client.capture).not.toHaveBeenCalled(); expect(client.group).not.toHaveBeenCalled();
    expect(client.setPersonProperties).not.toHaveBeenCalled();
    analytics.setAnalyticsContext({ workspaceId: "B-workspace", workspaceCount: 2 });
    analytics.captureProductEvent(EVENTS.fleet_viewed, { fleet_id: "B-fleet", status: "running" });
    expect(client.capture).toHaveBeenCalledExactlyOnceWith(EVENTS.fleet_viewed, { path: "/workspaces", fleet_id: "B-fleet", status: "running" });
    expect(client.group).toHaveBeenCalledExactlyOnceWith("workspace", "B-workspace");
    expect(client.setPersonProperties).toHaveBeenCalledExactlyOnceWith({ workspace_count: 2 });
  });

  it("discards A telemetry when B replaces the reader while the actual SDK import is pending", async () => {
    let release!: () => void; let loading!: () => void;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    const started = new Promise<void>((resolve) => { loading = resolve; });
    vi.doMock("posthog-js", async () => { loading(); await gate; return { default: client }; });
    const analytics = await import("../lib/analytics/posthog");
    const startup = analytics.initAnalytics(() => ({ id: "account-A" })); await started;
    analytics.captureProductEvent(EVENTS.fleet_created, { fleet_id: "A-fleet" });
    analytics.setAnalyticsContext({ workspaceId: "A-workspace", plan: "A-plan" });
    const replacement = analytics.initAnalytics(() => ({ id: "account-B" }));
    release(); await Promise.all([startup, replacement]);
    expect(client.identify).toHaveBeenCalledExactlyOnceWith("account-B", { user_id: "account-B" });
    expect(client.capture).not.toHaveBeenCalled(); expect(client.group).not.toHaveBeenCalled();
    expect(client.setPersonProperties).not.toHaveBeenCalled(); expect(client.init).toHaveBeenCalledTimes(1);
  });

  it("preserves A's queue across same-owner navigation, ignoring telemetry while the reader is cancelled", async () => {
    let resume!: () => void;
    vi.stubGlobal("requestIdleCallback", (callback: () => void) => { resume = callback; return 1; });
    const analytics = await import("../lib/analytics/posthog");
    const startup = analytics.initAnalytics(() => ({ id: "account-A" }));
    analytics.captureProductEvent(EVENTS.fleet_created, { fleet_id: "A-fleet" });
    analytics.setAnalyticsContext({ workspaceId: "A-workspace" });
    void analytics.initAnalytics(() => undefined);
    analytics.captureProductEvent(EVENTS.fleet_created, { fleet_id: "unowned-fleet" });
    analytics.setAnalyticsContext({ workspaceId: "unowned-workspace", plan: "unowned-plan" });
    window.history.replaceState(null, "", "/workspaces/next");
    const replacement = analytics.initAnalytics(() => ({ id: "account-A" }));
    resume(); await Promise.all([startup, replacement]);
    expect(client.capture).toHaveBeenCalledExactlyOnceWith(EVENTS.fleet_created, { path: "/workspaces", fleet_id: "A-fleet" });
    expect(client.group).toHaveBeenCalledExactlyOnceWith("workspace", "A-workspace");
    expect(client.setPersonProperties).not.toHaveBeenCalled();
  });

  it("keeps fresh B mount events without merging A's queued context during the same initial load", async () => {
    let resume!: () => void;
    vi.stubGlobal("requestIdleCallback", (callback: () => void) => { resume = callback; return 1; });
    const analytics = await import("../lib/analytics/posthog");
    const startup = analytics.initAnalytics(() => ({ id: "account-A" }));
    analytics.setAnalyticsContext({ workspaceId: "A-workspace", plan: "A-plan" });
    analytics.captureProductEvent(EVENTS.fleet_created, { fleet_id: "A-fleet" });
    const replacement = analytics.initAnalytics(() => ({ id: "account-B" }));
    analytics.setAnalyticsContext({ workspaceId: "B-workspace", workspaceCount: 2 });
    analytics.captureProductEvent(EVENTS.fleet_created, { fleet_id: "B-fleet" });
    resume(); await Promise.all([startup, replacement]);
    expect(client.capture).toHaveBeenCalledExactlyOnceWith(EVENTS.fleet_created, { path: "/workspaces", fleet_id: "B-fleet" });
    expect(client.group).toHaveBeenCalledExactlyOnceWith("workspace", "B-workspace");
    expect(client.setPersonProperties).toHaveBeenCalledExactlyOnceWith({ workspace_count: 2 });
  });

  it.each(["unmanaged", "account-A"])("drops the %s queue when an authoritative signed-out provider takes over", async (owner) => {
    let resume!: () => void;
    vi.stubGlobal("requestIdleCallback", (callback: () => void) => { resume = callback; return 1; });
    const analytics = await import("../lib/analytics/posthog");
    const startup = analytics.initAnalytics(owner === "unmanaged" ? undefined : () => ({ id: owner }));
    analytics.setAnalyticsContext({ workspaceId: "old-workspace" });
    analytics.captureProductEvent(EVENTS.fleet_created, { fleet_id: "old-fleet" });
    const replacement = analytics.initAnalytics(() => null);
    resume(); await Promise.all([startup, replacement]);
    expect(client.capture).not.toHaveBeenCalled(); expect(client.group).not.toHaveBeenCalled();
    analytics.captureProductEvent(EVENTS.fleet_created, { fleet_id: "anonymous-fleet" });
    expect(client.capture).toHaveBeenCalledExactlyOnceWith(EVENTS.fleet_created, { path: "/workspaces", fleet_id: "anonymous-fleet" });
  });

  it("stops synchronization if the identity provider becomes unavailable during the ownership check", async () => {
    const analytics = await import("../lib/analytics/posthog");
    await analytics.initAnalytics(() => ({ id: "account-A" }));
    let available = true;
    await analytics.initAnalytics(() => {
      const identity = available ? { id: "account-B" } : undefined;
      available = false;
      return identity;
    });
    expect(client.identify.mock.calls.map(([id]) => id)).toEqual(["account-A"]);
    const filter = client.init.mock.calls[0]![1].before_send as (event: unknown) => unknown;
    expect(filter({ event: "$pageview", properties: {} })).toBeNull();
    await analytics.initAnalytics(() => ({ id: "account-B" }));
    expect(client.identify.mock.calls.map(([id]) => id)).toEqual(["account-A", "account-B"]);
  });
});
afterEach(() => { vi.doUnmock("posthog-js"); vi.unstubAllEnvs(); vi.unstubAllGlobals(); });

describe("authentication telemetry exclusion", () => {
  it.each(["/cli-auth/session", "/sign-in", "/sign-up/continue"])("never initializes on %s", async (path) => {
    window.history.replaceState(null, "", path);
    const analytics = await import("../lib/analytics/posthog");
    await analytics.initAnalytics();
    expect(client.init).not.toHaveBeenCalled();
    analytics.trackAppEvent("test-event", { error_message: "sensitive callback" });
    expect(client.capture).not.toHaveBeenCalled();
  });

  it("blocks manual and automatic events after navigating from an initialized dashboard", async () => {
    const analytics = await import("../lib/analytics/posthog");
    await analytics.initAnalytics();
    const filter = client.init.mock.calls[0]![1].before_send as (event: unknown) => unknown;
    analytics.trackAppEvent("allowed-event");
    expect(client.capture).toHaveBeenCalledTimes(1);
    client.capture.mockClear();
    window.history.replaceState(null, "", "/cli-auth/session?flow=loopback#state=sensitive");
    analytics.trackAppEvent("blocked-event", { error_message: "sensitive" });
    analytics.identifyAnalyticsUser({ id: "person" });
    analytics.setAnalyticsContext({ workspaceId: "workspace" });
    expect(client.capture).not.toHaveBeenCalled();
    expect(client.identify).not.toHaveBeenCalled();
    expect(client.group).not.toHaveBeenCalled();
    expect(filter({ event: "$pageview", properties: {} })).toBeNull();
    expect(filter({ event: "$pageleave", properties: {} })).toBeNull();
  });

  it("rejects queued, nested and encoded auth URLs even after leaving the auth route", async () => {
    const analytics = await import("../lib/analytics/posthog");
    await analytics.initAnalytics();
    const filter = client.init.mock.calls[0]![1].before_send as (event: unknown) => unknown;
    for (const path of ["/cli-auth/session#state=secret", "/sign-in?redirect_url=%2Fcli-auth%2Fsession", "%252Fcli-auth%252Fsession"]) {
      analytics.trackAppEvent("navigation", { target: path });
      expect(filter({ properties: { nested: [{ href: path }] } })).toBeNull();
    }
    expect(client.capture).not.toHaveBeenCalled();
    const safe = { event: "$pageview", properties: { $current_url: `${ORIGIN}/workspaces` } };
    expect(filter(safe)).toBe(safe);
  });

  it("rechecks the route when idle startup resumes", async () => {
    let resume: () => void = () => {};
    vi.stubGlobal("requestIdleCallback", (callback: () => void) => { resume = callback; return 1; });
    const analytics = await import("../lib/analytics/posthog");
    const initialization = analytics.initAnalytics();
    window.history.replaceState(null, "", "/sign-in");
    resume(); await initialization;
    expect(client.init).not.toHaveBeenCalled();
    window.history.replaceState(null, "", "/workspaces");
    const retry = analytics.initAnalytics(); resume(); await retry;
    expect(client.init).toHaveBeenCalledTimes(1);
  });

  it("does not initialize an imported SDK on auth and retries after returning", async () => {
    let release!: () => void;
    let loading!: () => void;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    const started = new Promise<void>((resolve) => { loading = resolve; });
    vi.doMock("posthog-js", async () => { loading(); await gate; return { default: client }; });
    const analytics = await import("../lib/analytics/posthog");
    const initialization = analytics.initAnalytics(); await started;
    window.history.replaceState(null, "", "/cli-auth/session");
    release(); await initialization;
    expect(client.init).not.toHaveBeenCalled(); expect(client.capture).not.toHaveBeenCalled();
    window.history.replaceState(null, "", "/workspaces");
    await analytics.initAnalytics();
    expect(client.init).toHaveBeenCalledTimes(1);
  });

  it("keeps reset, newer identity and context ordering across an interrupted load", async () => {
    let resume!: () => void;
    vi.stubGlobal("requestIdleCallback", (callback: () => void) => { resume = callback; return 1; });
    const analytics = await import("../lib/analytics/posthog");
    const initialization = analytics.initAnalytics();
    analytics.identifyAnalyticsUser({ id: "old-person" });
    analytics.trackAppEvent("old-session-event");
    analytics.resetAnalyticsIdentity();
    analytics.identifyAnalyticsUser({ id: "current-person" });
    analytics.setAnalyticsContext({ workspaceId: "current-workspace" });
    window.history.replaceState(null, "", "/sign-in"); resume(); await initialization;
    window.history.replaceState(null, "", "/workspaces");
    const retry = analytics.initAnalytics(); resume(); await retry;
    expect(client.reset).toHaveBeenCalledTimes(1);
    expect(client.identify).toHaveBeenCalledExactlyOnceWith("current-person", { user_id: "current-person" });
    expect(client.group).not.toHaveBeenCalled(); // The deferred reset clears context from the prior session.
    expect(client.capture).not.toHaveBeenCalled();
    expect(client.reset.mock.invocationCallOrder[0]).toBeLessThan(client.identify.mock.invocationCallOrder[0]!);
    analytics.setAnalyticsContext({ workspaceId: "current-workspace" });
    expect(client.group).toHaveBeenCalledWith("workspace", "current-workspace");
  });

  it("contains an SDK initialization exception and allows a later attempt", async () => {
    client.init.mockImplementationOnce(() => { throw new Error("SDK blocked"); });
    const analytics = await import("../lib/analytics/posthog");
    await expect(analytics.initAnalytics()).resolves.toBeUndefined();
    expect(client.capture).not.toHaveBeenCalled();
    await analytics.initAnalytics();
    analytics.trackAppEvent("resumed-event");
    expect(client.init).toHaveBeenCalledTimes(2);
    expect(client.capture).toHaveBeenCalledWith("resumed-event", { path: "/workspaces" });
  });

  it("keeps a loaded client usable when the SDK rejects a queued identity", async () => {
    let resume!: () => void;
    vi.stubGlobal("requestIdleCallback", (callback: () => void) => { resume = callback; return 1; });
    client.identify.mockImplementationOnce(() => { throw new Error("Identity temporarily unavailable"); });
    const analytics = await import("../lib/analytics/posthog");
    const initialization = analytics.initAnalytics();
    analytics.identifyAnalyticsUser({ id: "current-person" });
    resume(); await initialization;
    analytics.identifyAnalyticsUser({ id: "current-person" });
    analytics.trackAppEvent("client-still-usable");
    expect(client.init).toHaveBeenCalledTimes(1);
    expect(client.identify).toHaveBeenCalledTimes(2);
    expect(client.capture).toHaveBeenCalledWith("client-still-usable", { path: "/workspaces" });
  });

  it("recognizes route boundaries without excluding similarly named product pages", () => {
    expect(isSensitiveAuthUrl("/workspaces/sign-in-guide")).toBe(false);
    expect(isSensitiveAuthUrl("/cli-authentication")).toBe(false);
    expect(isSensitiveAuthUrl("%2Fcli-auth%2Fsession")).toBe(true);
    expect(isSensitiveAuthUrl("%GG")).toBe(false);
  });

  it("fails closed when the installed identity reader throws before or after SDK load", async () => {
    const analytics = await import("../lib/analytics/posthog");
    const unavailable = () => { throw new Error("Identity unavailable"); };
    await analytics.initAnalytics(unavailable);
    expect(client.init).not.toHaveBeenCalled();
    await analytics.initAnalytics(() => ({ id: "current-person" }));
    const filter = client.init.mock.calls[0]![1].before_send as (event: unknown) => unknown;
    await analytics.initAnalytics(unavailable);
    expect(filter({ event: "$autocapture", properties: {} })).toBeNull();
    analytics.trackAppEvent("held-event"); expect(client.capture).not.toHaveBeenCalled();
    await analytics.initAnalytics(() => ({ id: "current-person" }));
    expect(client.capture).not.toHaveBeenCalled();
    analytics.trackAppEvent("fresh-owned-event");
    expect(client.capture).toHaveBeenCalledExactlyOnceWith("fresh-owned-event", { path: "/workspaces" });
  });

  it("bounds traversal and handles cycles without emitting oversized payloads", () => {
    const cyclic: { self?: unknown } = {}; cyclic.self = cyclic;
    expect(filterAuthenticationEvent(cyclic)).toBe(cyclic);
    expect(filterAuthenticationEvent(Array.from({ length: 1_001 }, () => 1))).toBeNull();
  });
});
