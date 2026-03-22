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
      addEventListener: vi.fn((name: string, handler: EventListener) => {
        listeners.set(name, handler);
      }),
      __listeners: listeners,
    });
  });

  it("initializes analytics and registers runtime listeners on import", async () => {
    await import("../instrumentation-client");

    expect(initAnalytics).toHaveBeenCalledTimes(1);

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

  it("swallows instrumentation startup failures", async () => {
    vi.resetModules();
    initAnalytics.mockImplementation(() => {
      throw new Error("init failed");
    });
    vi.stubGlobal("window", {
      addEventListener: vi.fn(() => {
        throw new Error("listener failed");
      }),
    });

    await expect(import("../instrumentation-client")).resolves.toBeTruthy();
  });
});
