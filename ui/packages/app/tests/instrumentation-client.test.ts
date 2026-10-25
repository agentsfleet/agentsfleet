import { beforeEach, describe, expect, it, vi } from "vitest";

const initAnalytics = vi.fn();
const trackAppEvent = vi.fn();

vi.mock("../lib/analytics/posthog", () => ({
  initAnalytics,
  trackAppEvent,
}));

describe("instrumentation-client", () => {
  beforeEach(() => {
    vi.resetModules();
    initAnalytics.mockReset();
    trackAppEvent.mockReset();

    const listeners = new Map<string, EventListener>();
    vi.stubGlobal("window", {
      location: { href: "https://app.example.test/", pathname: "/" },
      addEventListener: vi.fn((name: string, handler: EventListener) => {
        listeners.set(name, handler);
      }),
      __listeners: listeners,
    });
  });

  it("initializes analytics and registers runtime listeners on import", async () => {
    await import("../instrumentation-client");

    expect(initAnalytics).toHaveBeenCalledTimes(1);
    expect(initAnalytics.mock.calls[0]![0]()).toBeUndefined(); // Bootstrap must replace the startup hold.

    const listeners = (window as typeof window & { __listeners: Map<string, EventListener> }).__listeners;
    listeners.get("error")?.({
      message: "boom",
      error: new Error("boom"),
    } as unknown as Event);
    listeners.get("error")?.({
      message: "",
      error: undefined,
    } as unknown as Event);
    listeners.get("unhandledrejection")?.({
      reason: new Error("reject"),
    } as unknown as Event);
    listeners.get("unhandledrejection")?.({
      reason: "string-reject",
    } as unknown as Event);
    listeners.get("unhandledrejection")?.({
      reason: { code: "E_FAIL" },
    } as unknown as Event);

    expect(trackAppEvent).toHaveBeenCalledWith("ui_runtime_error", expect.objectContaining({
      source: "window_error",
      surface: "browser_runtime",
    }));
    expect(trackAppEvent).toHaveBeenCalledWith("ui_runtime_error", expect.objectContaining({
      source: "window_unhandled_rejection",
      surface: "browser_runtime",
      error_message: "reject",
    }));
    expect(trackAppEvent).toHaveBeenCalledWith("ui_runtime_error", expect.objectContaining({
      source: "window_unhandled_rejection",
      error_message: "string-reject",
    }));
    expect(trackAppEvent).toHaveBeenCalledWith("ui_runtime_error", expect.objectContaining({
      source: "window_unhandled_rejection",
      error_message: JSON.stringify({ code: "E_FAIL" }),
    }));
  });

  it("tracks router transition start", async () => {
    const mod = await import("../instrumentation-client");
    mod.onRouterTransitionStart("/workspaces/ws_123", "push");

    expect(trackAppEvent).toHaveBeenCalledWith("page_navigation_started", {
      source: "app_router",
      surface: "navigation",
      target: "/workspaces/ws_123",
      reason: "push",
    });
  });

  it.each(["/cli-auth/session", "/sign-in", "/sign-up/continue"])("keeps startup and runtime failures private on %s", async (path) => {
    window.location.pathname = path;
    const mod = await import("../instrumentation-client");
    expect(initAnalytics).not.toHaveBeenCalled();
    const listeners = (window as typeof window & { __listeners: Map<string, EventListener> }).__listeners;
    const sensitiveError = vi.fn(() => { throw new Error("Auth error details must not be read"); });
    const sensitiveRejection = vi.fn(() => { throw new Error("Auth rejection details must not be read"); });
    listeners.get("error")?.({ get message() { return sensitiveError(); } } as unknown as Event);
    listeners.get("unhandledrejection")?.({ get reason() { return sensitiveRejection(); } } as unknown as Event);
    mod.onRouterTransitionStart("/workspaces", "replace");
    expect(sensitiveError).not.toHaveBeenCalled(); expect(sensitiveRejection).not.toHaveBeenCalled();
    expect(trackAppEvent).not.toHaveBeenCalled();
  });

  it("rechecks the current route for existing listeners after dashboard-to-auth navigation", async () => {
    await import("../instrumentation-client");
    expect(initAnalytics.mock.calls[0]![0]()).toBeUndefined();
    window.location.pathname = "/cli-auth/session";
    const listeners = (window as typeof window & { __listeners: Map<string, EventListener> }).__listeners;
    listeners.get("error")?.({ message: "private callback details" } as unknown as Event);
    listeners.get("unhandledrejection")?.({ reason: "private rejection details" } as unknown as Event);
    expect(trackAppEvent).not.toHaveBeenCalled();
    window.location.pathname = "/workspaces";
    listeners.get("error")?.({ message: "product failure" } as unknown as Event);
    expect(trackAppEvent).toHaveBeenCalledExactlyOnceWith("ui_runtime_error", {
      source: "window_error", surface: "browser_runtime", error_message: "product failure",
    });
  });

  it.each(["/cli-auth/session?flow=loopback#state=private", "/sign-in", "%2Fsign-up%2Fcontinue"])("does not forward an auth destination from the dashboard: %s", async (target) => {
    const mod = await import("../instrumentation-client");
    mod.onRouterTransitionStart(target, "push");
    expect(trackAppEvent).not.toHaveBeenCalled();
  });

  it("swallows instrumentation startup failures", async () => {
    vi.resetModules();
    initAnalytics.mockImplementation(() => {
      throw new Error("init failed");
    });
    vi.stubGlobal("window", {
      location: { href: "https://app.example.test/", pathname: "/" },
      addEventListener: vi.fn(() => {
        throw new Error("listener failed");
      }),
    });

    await expect(import("../instrumentation-client")).resolves.toBeTruthy();
  });
});
