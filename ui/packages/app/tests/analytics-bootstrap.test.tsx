import { act, cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const boundary = vi.hoisted(() => ({
  pathname: "/sign-in", user: { isLoaded: true, isSignedIn: true, userId: "first-person", emailAddress: "first@example.test" },
  load: vi.fn(), init: vi.fn(), capture: vi.fn(), identify: vi.fn(), reset: vi.fn(), group: vi.fn(), setPersonProperties: vi.fn(),
}));
vi.mock("next/navigation", () => ({ usePathname: () => boundary.pathname }));
vi.mock("@/lib/auth/client", () => ({ useCurrentUser: () => boundary.user }));
const idle: Array<() => void> = [];

beforeEach(() => {
  vi.resetModules(); vi.resetAllMocks(); idle.length = 0;
  vi.doMock("posthog-js", () => {
    boundary.load();
    return { default: { init: boundary.init, capture: boundary.capture, identify: boundary.identify, reset: boundary.reset, group: boundary.group, setPersonProperties: boundary.setPersonProperties } };
  });
  boundary.pathname = "/sign-in";
  boundary.user = { isLoaded: true, isSignedIn: true, userId: "first-person", emailAddress: "first@example.test" };
  window.history.replaceState(null, "", boundary.pathname);
  window.localStorage.clear();
  vi.stubEnv("NEXT_PUBLIC_POSTHOG_KEY", "test-public-project-key");
  vi.stubEnv("NEXT_PUBLIC_POSTHOG_ENABLED", "true");
  vi.stubGlobal("requestIdleCallback", (callback: () => void) => { idle.push(callback); return idle.length; });
});

describe("identity before queued telemetry", () => {
  function persistedAccount(automatic = false) {
    let account = "persisted-A";
    let filter: (event: unknown) => unknown = (event) => event;
    const delivered: Array<{ kind: string; account: string }> = [];
    boundary.identify.mockImplementation((id: string) => {
      account = id;
      if (automatic && filter({ event: "$identify", properties: { distinct_id: account } })) delivered.push({ kind: "$identify", account });
    });
    boundary.reset.mockImplementation(() => { account = "anonymous"; });
    boundary.capture.mockImplementation(() => { delivered.push({ kind: "event", account }); });
    boundary.group.mockImplementation(() => { delivered.push({ kind: "group", account }); });
    boundary.setPersonProperties.mockImplementation(() => { delivered.push({ kind: "properties", account }); });
    if (automatic) boundary.init.mockImplementation((_key: string, options: Record<string, unknown>) => {
      filter = options.before_send as typeof filter;
      (options.loaded as ((client: typeof boundary) => void) | undefined)?.(boundary);
      if (filter({ event: "$pageview", properties: { distinct_id: account } })) delivered.push({ kind: "$pageview", account });
    });
    window.localStorage.setItem("uz_analytics_identified", "1");
    boundary.pathname = "/workspaces"; window.history.replaceState(null, "", boundary.pathname);
    return delivered;
  }

  async function mountAndQueue() {
    const analytics = await import("../lib/analytics/posthog");
    void analytics.initAnalytics(); // Instrumentation can start before the bootstrap mounts.
    const Bootstrap = (await import("../components/analytics/AnalyticsBootstrap")).default;
    const view = render(<Bootstrap />);
    analytics.setAnalyticsContext({ workspaceId: "B-workspace", workspaceCount: 1 });
    analytics.trackAppEvent("mount-event");
    return { analytics, view, Bootstrap };
  }

  it("binds current Clerk B before any queued group, person properties or event reaches persisted A", async () => {
    const delivered = persistedAccount(); boundary.user = { ...boundary.user, userId: "current-B" };
    await mountAndQueue(); await releaseIdle();
    expect(delivered).toEqual([
      { kind: "group", account: "current-B" }, { kind: "properties", account: "current-B" }, { kind: "event", account: "current-B" },
    ]);
  });

  it("resets a signed-out stale account before any queued telemetry can escape", async () => {
    const delivered = persistedAccount(); boundary.user = { ...boundary.user, isSignedIn: false };
    const { analytics } = await mountAndQueue(); await releaseIdle();
    expect(boundary.reset).toHaveBeenCalledTimes(1);
    expect(delivered).toEqual([]);
    analytics.trackAppEvent("anonymous-after-reset");
    expect(delivered).toEqual([{ kind: "event", account: "anonymous" }]);
  });

  it("discards unowned telemetry while Clerk is unresolved and accepts fresh B telemetry after resolution", async () => {
    const delivered = persistedAccount(); boundary.user = { ...boundary.user, isLoaded: false };
    const { analytics, view, Bootstrap } = await mountAndQueue(); await releaseIdle();
    analytics.trackAppEvent("still-unresolved"); analytics.setAnalyticsContext({ workspaceCount: 2 });
    expect(delivered).toEqual([]); expect(boundary.identify).not.toHaveBeenCalled();
    boundary.user = { ...boundary.user, isLoaded: true, userId: "current-B" };
    await act(async () => { view.rerender(<Bootstrap />); }); await releaseIdle();
    expect(delivered).toEqual([]);
    analytics.setAnalyticsContext({ workspaceId: "B-workspace" }); analytics.trackAppEvent("fresh-B-event");
    expect(delivered).toEqual([{ kind: "group", account: "current-B" }, { kind: "event", account: "current-B" }]);
  });

  it("does not release queued telemetry after unmount, and resumes with the replacement account", async () => {
    const delivered = persistedAccount();
    const { analytics, view, Bootstrap } = await mountAndQueue(); view.unmount(); await releaseIdle();
    analytics.trackAppEvent("between-mounts");
    expect(delivered).toEqual([]); expect(boundary.identify).not.toHaveBeenCalled();
    boundary.user = { ...boundary.user, userId: "replacement-B" };
    await act(async () => { render(<Bootstrap />); }); await releaseIdle();
    expect(delivered).toEqual([]);
    analytics.trackAppEvent("fresh-replacement-event");
    expect(delivered).toEqual([{ kind: "event", account: "replacement-B" }]);
  });

  it("replaces a cancelled auth navigation callback before releasing queues on return", async () => {
    const delivered = persistedAccount();
    const { analytics, view, Bootstrap } = await mountAndQueue();
    await move("/sign-in", () => view.rerender(<Bootstrap />)); await releaseIdle();
    analytics.trackAppEvent("auth-must-not-escape");
    expect(delivered).toEqual([]); expect(boundary.load).not.toHaveBeenCalled();
    boundary.user = { ...boundary.user, userId: "returned-B" };
    await move("/workspaces", () => view.rerender(<Bootstrap />)); await releaseIdle();
    expect(delivered).toEqual([]);
    analytics.trackAppEvent("fresh-return-event");
    expect(delivered).toEqual([{ kind: "event", account: "returned-B" }]);
  });

  it("applies B in the SDK loaded callback before its initial pageview without recursive identify", async () => {
    const delivered = persistedAccount(true); boundary.user = { ...boundary.user, userId: "current-B" };
    await mountAndQueue(); await releaseIdle();
    expect(delivered.find(({ kind }) => kind === "$pageview")).toEqual({ kind: "$pageview", account: "current-B" });
    expect(delivered.find(({ kind }) => kind === "$identify")).toEqual({ kind: "$identify", account: "current-B" });
    expect(boundary.identify).toHaveBeenCalledTimes(1);
    expect(delivered.every(({ account }) => account === "current-B")).toBe(true);
  });

  it("blocks the initial SDK pageview when identity synchronization fails instead of exposing persisted A", async () => {
    const delivered = persistedAccount(true); boundary.user = { ...boundary.user, userId: "current-B" };
    boundary.identify.mockImplementationOnce(() => { throw new Error("SDK identify failed"); });
    await mountAndQueue(); await releaseIdle();
    expect(delivered.some(({ account }) => account === "persisted-A")).toBe(false);
    expect(delivered.some(({ kind }) => kind === "$pageview")).toBe(false);
    expect(delivered.some(({ kind, account }) => kind === "event" && account === "current-B")).toBe(true);
  });

  it("keeps signed-out delivery closed until a failed SDK reset succeeds", async () => {
    const delivered = persistedAccount(true); boundary.user = { ...boundary.user, isSignedIn: false };
    boundary.reset.mockImplementationOnce(() => { throw new Error("SDK reset failed"); });
    const { analytics } = await mountAndQueue(); await releaseIdle();
    expect(boundary.reset).toHaveBeenCalledTimes(2);
    expect(delivered).toEqual([]);
    analytics.trackAppEvent("anonymous-after-retry");
    expect(delivered).toEqual([{ kind: "event", account: "anonymous" }]);
  });

  it("does not load a persisted SDK until Clerk resolves and rejects SDK events during later unresolved or cancelled effects", async () => {
    persistedAccount(true); boundary.user = { ...boundary.user, isLoaded: false };
    const { view, Bootstrap } = await mountAndQueue(); await releaseIdle();
    expect(boundary.load).not.toHaveBeenCalled(); expect(boundary.init).not.toHaveBeenCalled();
    boundary.user = { ...boundary.user, isLoaded: true, userId: "current-B" };
    await act(async () => { view.rerender(<Bootstrap />); }); await releaseIdle();
    const filter = boundary.init.mock.calls[0]![1].before_send as (event: unknown) => unknown;
    const pageleave = { event: "$pageleave", properties: { distinct_id: "current-B" } };
    expect(filter(pageleave)).toBe(pageleave);
    boundary.user = { ...boundary.user, isLoaded: false };
    await act(async () => { view.rerender(<Bootstrap />); });
    expect(filter(pageleave)).toBeNull();
    expect(filter({ event: "$autocapture", properties: {} })).toBeNull();
    view.unmount(); expect(filter(pageleave)).toBeNull();
  });

  it("contains a context SDK failure after identity recovery and retries remaining owned events", async () => {
    const delivered = persistedAccount();
    boundary.identify.mockImplementationOnce(() => { throw new Error("SDK identity unavailable"); });
    const { view, Bootstrap } = await mountAndQueue(); await releaseIdle();
    boundary.group.mockImplementationOnce(() => { throw new Error("SDK context unavailable"); });
    boundary.user = { ...boundary.user, emailAddress: "recovering@example.test" };
    await act(async () => { view.rerender(<Bootstrap />); });
    expect(delivered).toEqual([]);
    boundary.user = { ...boundary.user, emailAddress: "updated@example.test" };
    await act(async () => { view.rerender(<Bootstrap />); });
    expect(delivered).toEqual([{ kind: "event", account: "first-person" }]);
  });

  it("holds real instrumentation startup before Bootstrap supplies an identity", async () => {
    persistedAccount(true);
    await import("../instrumentation-client"); await releaseIdle();
    expect(boundary.load).not.toHaveBeenCalled(); expect(boundary.init).not.toHaveBeenCalled();
    const Bootstrap = (await import("../components/analytics/AnalyticsBootstrap")).default;
    render(<Bootstrap />); await releaseIdle();
    expect(boundary.init).toHaveBeenCalledTimes(1); expect(boundary.identify).toHaveBeenCalledTimes(1);
  });

  it.each([false, true])("makes the current provider authoritative over a legacy queued identity (reset=%s)", async (reset) => {
    const delivered = persistedAccount(); boundary.user = { ...boundary.user, userId: "current-B" };
    const analytics = await import("../lib/analytics/posthog");
    void analytics.initAnalytics();
    analytics.identifyAnalyticsUser({ id: "legacy-A" });
    analytics.setAnalyticsContext({ workspaceId: "legacy-workspace" }); analytics.trackAppEvent("legacy-event");
    if (reset) analytics.resetAnalyticsIdentity();
    const Bootstrap = (await import("../components/analytics/AnalyticsBootstrap")).default;
    render(<Bootstrap />); await releaseIdle();
    expect(delivered).toEqual([]);
    expect(boundary.identify.mock.calls.map(([id]) => id)).toEqual(["current-B"]);
    expect(boundary.reset.mock.invocationCallOrder[0]).toBeLessThan(boundary.identify.mock.invocationCallOrder[0]!);
    analytics.setAnalyticsContext({ workspaceId: "B-workspace" }); analytics.trackAppEvent("B-event");
    expect(delivered).toEqual([{ kind: "group", account: "current-B" }, { kind: "event", account: "current-B" }]);
  });

  it("discards queued context and events from a previously identified account when the provider changes", async () => {
    const delivered = persistedAccount();
    const { analytics, view, Bootstrap } = await mountAndQueue(); await releaseIdle(); delivered.length = 0;
    boundary.user = { ...boundary.user, isLoaded: false };
    await act(async () => { view.rerender(<Bootstrap />); });
    analytics.setAnalyticsContext({ workspaceId: "prior-workspace" }); analytics.trackAppEvent("prior-event");
    boundary.user = { ...boundary.user, isLoaded: true, userId: "new-B" };
    await act(async () => { view.rerender(<Bootstrap />); });
    expect(delivered).toEqual([]); expect(boundary.reset).toHaveBeenCalledTimes(1);
    analytics.setAnalyticsContext({ workspaceId: "new-workspace" }); analytics.trackAppEvent("new-event");
    expect(delivered).toEqual([{ kind: "group", account: "new-B" }, { kind: "event", account: "new-B" }]);
  });
});
afterEach(() => { cleanup(); vi.doUnmock("posthog-js"); vi.unstubAllGlobals(); vi.unstubAllEnvs(); });

async function move(path: string, rerender: () => void): Promise<void> {
  boundary.pathname = path; window.history.replaceState(null, "", path);
  await act(async () => { rerender(); });
}

async function releaseIdle(): Promise<void> {
  await act(async () => { for (const resume of idle.splice(0)) resume(); });
}

describe("committed-route analytics startup", () => {
  it("initializes after a hard auth entry returns to the dashboard with the current identity", async () => {
    const Bootstrap = (await import("../components/analytics/AnalyticsBootstrap")).default;
    const view = render(<Bootstrap />);
    const analytics = await import("../lib/analytics/posthog");
    analytics.trackAppEvent("auth-must-not-capture");
    expect(boundary.load).not.toHaveBeenCalled(); expect(boundary.capture).not.toHaveBeenCalled();
    boundary.user = { ...boundary.user, userId: "signed-in-person", emailAddress: "signed-in@example.test" };
    await move("/workspaces", () => view.rerender(<Bootstrap />));
    await releaseIdle();
    await waitFor(() => expect(boundary.init).toHaveBeenCalledTimes(1));
    expect(boundary.identify).toHaveBeenCalledWith("signed-in-person", expect.objectContaining({ email: "signed-in@example.test" }));
    expect(boundary.load).toHaveBeenCalledTimes(1);
    analytics.trackAppEvent("dashboard-resumed");
    expect(boundary.capture).toHaveBeenCalledExactlyOnceWith("dashboard-resumed", { path: "/workspaces" });
  });

  it("retries idle startup interrupted by auth and never identifies the stale account", async () => {
    boundary.pathname = "/workspaces"; window.history.replaceState(null, "", boundary.pathname);
    const Bootstrap = (await import("../components/analytics/AnalyticsBootstrap")).default;
    const view = render(<Bootstrap />);
    await move("/sign-in", () => view.rerender(<Bootstrap />)); await releaseIdle();
    expect(boundary.load).not.toHaveBeenCalled(); expect(boundary.identify).not.toHaveBeenCalled();
    boundary.user = { ...boundary.user, userId: "second-person" };
    await move("/workspaces/next", () => view.rerender(<Bootstrap />)); await releaseIdle();
    await waitFor(() => expect(boundary.init).toHaveBeenCalledTimes(1));
    expect(boundary.identify.mock.calls.map(([id]) => id)).toEqual(["second-person"]);
  });

  it("does not identify after the bootstrap unmounts before initialization settles", async () => {
    boundary.pathname = "/workspaces"; window.history.replaceState(null, "", boundary.pathname);
    const Bootstrap = (await import("../components/analytics/AnalyticsBootstrap")).default;
    const view = render(<Bootstrap />); view.unmount(); await releaseIdle();
    expect(boundary.init).not.toHaveBeenCalled();
    expect(boundary.identify).not.toHaveBeenCalled();
  });

  it("does not apply a stale identity when the browser has moved before React commits the new route", async () => {
    boundary.pathname = "/workspaces"; window.history.replaceState(null, "", boundary.pathname);
    const Bootstrap = (await import("../components/analytics/AnalyticsBootstrap")).default;
    render(<Bootstrap />);
    window.history.replaceState(null, "", "/settings"); await releaseIdle();
    expect(boundary.init).not.toHaveBeenCalled();
    expect(boundary.identify).not.toHaveBeenCalled();
  });

  it("coalesces concurrent startup callers and applies only the latest mounted identity", async () => {
    boundary.pathname = "/workspaces"; window.history.replaceState(null, "", boundary.pathname);
    const analytics = await import("../lib/analytics/posthog");
    const first = analytics.initAnalytics(); const second = analytics.initAnalytics();
    const Bootstrap = (await import("../components/analytics/AnalyticsBootstrap")).default;
    const view = render(<Bootstrap />);
    boundary.user = { ...boundary.user, userId: "latest-person" };
    view.rerender(<Bootstrap />); await releaseIdle(); await Promise.all([first, second]);
    await waitFor(() => expect(boundary.identify).toHaveBeenCalledWith("latest-person", expect.any(Object)));
    expect(boundary.init).toHaveBeenCalledTimes(1);
    expect(boundary.identify.mock.calls.map(([id]) => id)).toEqual(["latest-person"]);
  });

  it("resets an old identity only after a signed-out dashboard can initialize", async () => {
    boundary.user = { ...boundary.user, isSignedIn: false };
    window.localStorage.setItem("uz_analytics_identified", "1");
    const Bootstrap = (await import("../components/analytics/AnalyticsBootstrap")).default;
    const view = render(<Bootstrap />);
    expect(boundary.reset).not.toHaveBeenCalled();
    await move("/workspaces", () => view.rerender(<Bootstrap />)); await releaseIdle();
    await waitFor(() => expect(boundary.reset).toHaveBeenCalledTimes(1));
    expect(boundary.identify).not.toHaveBeenCalled();
  });

  it("ignores an unloaded user and contains identity SDK exceptions", async () => {
    boundary.pathname = "/workspaces"; window.history.replaceState(null, "", boundary.pathname);
    boundary.user = { ...boundary.user, isLoaded: false };
    const Bootstrap = (await import("../components/analytics/AnalyticsBootstrap")).default;
    const view = render(<Bootstrap />); await releaseIdle();
    expect(boundary.identify).not.toHaveBeenCalled();
    boundary.identify.mockImplementationOnce(() => { throw new Error("SDK refused identity"); });
    boundary.user = { ...boundary.user, isLoaded: true };
    view.rerender(<Bootstrap />); await releaseIdle();
    await waitFor(() => expect(boundary.identify).toHaveBeenCalledTimes(1));
  });

  it.each([
    { isSignedIn: true, userId: "", label: "unresolved signed-in identity" },
    { isSignedIn: false, userId: "", label: "anonymous visitor without a prior identity" },
  ])("does not identify or reset an $label", async ({ isSignedIn, userId }) => {
    boundary.pathname = "/workspaces"; window.history.replaceState(null, "", boundary.pathname);
    boundary.user = { ...boundary.user, isSignedIn, userId };
    const Bootstrap = (await import("../components/analytics/AnalyticsBootstrap")).default;
    render(<Bootstrap />); await releaseIdle();
    expect(boundary.init).toHaveBeenCalledTimes(isSignedIn ? 0 : 1);
    expect(boundary.identify).not.toHaveBeenCalled(); expect(boundary.reset).not.toHaveBeenCalled();
  });
});
