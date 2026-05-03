// @vitest-environment node
// Stays on the node environment because two tests below exercise the
// "no window / server-side" branch of the analytics module. Under happy-dom
// `window` is always defined, which defeats that assertion.
import { beforeEach, describe, expect, it, vi } from "vitest";

type MockClient = {
  init: ReturnType<typeof vi.fn>;
  capture: ReturnType<typeof vi.fn>;
  identify: ReturnType<typeof vi.fn>;
};

function createWindow(pathname = "/workspaces") {
  return {
    location: { pathname },
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  };
}

async function loadModule(options: {
  env?: Record<string, string | undefined>;
  pathname?: string;
  withWindow?: boolean;
  client?: MockClient;
} = {}) {
  vi.resetModules();

  const client = options.client ?? {
    init: vi.fn(),
    capture: vi.fn(),
    identify: vi.fn(),
  };

  vi.doMock("posthog-js", () => ({
    default: client,
  }));

  if (options.withWindow === false) {
    vi.unstubAllGlobals();
  } else {
    vi.stubGlobal("window", createWindow(options.pathname));
  }

  vi.unstubAllEnvs();
  for (const [key, value] of Object.entries(options.env ?? {})) {
    if (value === undefined) continue;
    vi.stubEnv(key, value);
  }

  const mod = await import("../lib/analytics/posthog");
  return { mod, client };
}

describe("app analytics", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it("resolves enabled config when key is present", async () => {
    const { mod } = await loadModule();
    const cfg = mod.appAnalyticsInternals.resolveConfig({
      NEXT_PUBLIC_POSTHOG_KEY: "phc_test",
    });
    expect(cfg.enabled).toBe(true);
    expect(cfg.host).toBe("https://us.i.posthog.com");
  });

  it("bool env parsing disables analytics for explicit false values", async () => {
    const { mod } = await loadModule();
    const cfgFalse = mod.appAnalyticsInternals.resolveConfig({
      NEXT_PUBLIC_POSTHOG_KEY: "phc_test",
      NEXT_PUBLIC_POSTHOG_ENABLED: "false",
    });
    const cfgOff = mod.appAnalyticsInternals.resolveConfig({
      NEXT_PUBLIC_POSTHOG_KEY: "phc_test",
      NEXT_PUBLIC_POSTHOG_ENABLED: "off",
    });
    const cfgNo = mod.appAnalyticsInternals.resolveConfig({
      NEXT_PUBLIC_POSTHOG_KEY: "phc_test",
      NEXT_PUBLIC_POSTHOG_ENABLED: "no",
    });
    expect(cfgFalse.enabled).toBe(false);
    expect(cfgOff.enabled).toBe(false);
    expect(cfgNo.enabled).toBe(false);
  });

  it("sanitizes payload to allowed keys and non-empty values", async () => {
    const { mod } = await loadModule();
    const props = mod.appAnalyticsInternals.sanitizeProps({
      source: " sidebar ",
      surface: "app",
      target: "",
      path: "/workspaces",
      workspace_id: " ws_123 ",
      has_error: false,
      workspace_count: 3,
      error_message: null as never,
      ignored: "value",
    } as unknown as Parameters<typeof mod.appAnalyticsInternals.sanitizeProps>[0]);
    expect(props).toEqual({
      source: "sidebar",
      surface: "app",
      path: "/workspaces",
      workspace_id: "ws_123",
      has_error: false,
      workspace_count: 3,
    });
  });

  it("does not initialize analytics on the server", async () => {
    const { mod, client } = await loadModule({
      withWindow: false,
      env: {
        NEXT_PUBLIC_POSTHOG_KEY: "phc_test",
      },
    });

    await mod.initAnalytics();
    expect(client.init).not.toHaveBeenCalled();
  });

  it("does not initialize analytics when disabled", async () => {
    const { mod, client } = await loadModule({
      env: {
        NEXT_PUBLIC_POSTHOG_KEY: "phc_test",
        NEXT_PUBLIC_POSTHOG_ENABLED: "0",
      },
    });

    await mod.initAnalytics();
    expect(client.init).not.toHaveBeenCalled();
  });

  it("does not initialize analytics when enabled is forced without a key", async () => {
    const { mod, client } = await loadModule({
      env: {
        NEXT_PUBLIC_POSTHOG_ENABLED: "true",
      },
    });

    await mod.initAnalytics();
    expect(client.init).not.toHaveBeenCalled();
  });

  it("initializes posthog once with the resolved config", async () => {
    const { mod, client } = await loadModule({
      pathname: "/workspaces/ws_123",
      env: {
        NEXT_PUBLIC_POSTHOG_KEY: "phc_live",
        NEXT_PUBLIC_POSTHOG_HOST: "https://eu.i.posthog.com",
      },
    });

    await mod.initAnalytics();
    await mod.initAnalytics();

    expect(client.init).toHaveBeenCalledTimes(1);
    expect(client.init).toHaveBeenCalledWith("phc_live", expect.objectContaining({
      api_host: "https://eu.i.posthog.com",
      capture_pageview: true,
      capture_pageleave: true,
    }));
  });

  it("identifies the current user and suppresses duplicate identifies", async () => {
    const { mod, client } = await loadModule({
      env: {
        NEXT_PUBLIC_POSTHOG_KEY: "phc_live",
      },
    });

    await mod.initAnalytics();
    mod.identifyAnalyticsUser({ id: "user_123", email: "user@example.com" });
    mod.identifyAnalyticsUser({ id: "user_123", email: "user@example.com" });

    expect(client.identify).toHaveBeenCalledTimes(1);
    expect(client.identify).toHaveBeenCalledWith("user_123", {
      user_id: "user_123",
      email: "user@example.com",
    });
  });

  it("skips identify when the client has no identify method and handles null email", async () => {
    const { mod, client } = await loadModule({
      client: {
        init: vi.fn(),
        capture: vi.fn(),
        identify: undefined as unknown as ReturnType<typeof vi.fn>,
      },
      env: {
        NEXT_PUBLIC_POSTHOG_KEY: "phc_live",
      },
    });

    await mod.initAnalytics();
    mod.identifyAnalyticsUser({ id: "user_456", email: null });

    expect(client.init).toHaveBeenCalledTimes(1);
  });

  it("captures app events with the current pathname fallback", async () => {
    const { mod, client } = await loadModule({
      pathname: "/workspaces/ws_123",
      env: {
        NEXT_PUBLIC_POSTHOG_KEY: "phc_live",
      },
    });

    await mod.initAnalytics();
    mod.trackAppEvent("workspace_detail_viewed", {
      source: "workspace_page",
      surface: "workspace_detail",
      workspace_id: "ws_123",
      has_error: false,
    });

    expect(client.capture).toHaveBeenCalledWith("workspace_detail_viewed", {
      source: "workspace_page",
      surface: "workspace_detail",
      workspace_id: "ws_123",
      has_error: false,
      path: "/workspaces/ws_123",
    });
  });

  it("does not capture when the browser context disappears", async () => {
    const { mod, client } = await loadModule({
      pathname: "/workspaces/ws_123",
      env: {
        NEXT_PUBLIC_POSTHOG_KEY: "phc_live",
      },
    });

    await mod.initAnalytics();
    vi.unstubAllGlobals();
    mod.trackAppEvent("workspace_detail_viewed", {
      source: "workspace_page",
      surface: "workspace_detail",
    });

    expect(client.capture).not.toHaveBeenCalled();
  });

  it("tracks navigation clicks through the shared capture helper", async () => {
    const { mod, client } = await loadModule({
      pathname: "/workspaces",
      env: {
        NEXT_PUBLIC_POSTHOG_KEY: "phc_live",
      },
    });

    await mod.initAnalytics();
    mod.trackNavigationClicked({
      source: "sidebar",
      surface: "workspace_list",
      target: "/workspaces/ws_123",
    });

    expect(client.capture).toHaveBeenCalledWith("navigation_clicked", expect.objectContaining({
      source: "sidebar",
      surface: "workspace_list",
      target: "/workspaces/ws_123",
      path: "/workspaces",
    }));
  });
});
