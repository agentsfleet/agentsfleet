import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { act, render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { OWN_ACCOUNT } from "@/tests/helpers/workspace-fixtures";
const TOAST_FADE_MS = 240;
const mocks = vi.hoisted(() => ({
  trackAppEvent: vi.fn(), trackNavigationClicked: vi.fn(),
  identifyAnalyticsUser: vi.fn(), resetAnalyticsIdentity: vi.fn(),
  hasStaleAnalyticsIdentity: vi.fn(() => false), setAnalyticsContext: vi.fn(),
  captureProductEvent: vi.fn(), useUser: vi.fn(), usePathname: vi.fn(),
  initAnalytics: vi.fn(async (read?: () => { id: string; email?: string | null } | null | undefined) => {
    const identity = read?.();
    if (identity) mocks.identifyAnalyticsUser(identity);
    else if (identity === null && mocks.hasStaleAnalyticsIdentity()) mocks.resetAnalyticsIdentity();
  }),
}));

vi.mock("@/lib/analytics/posthog", () => ({
  trackAppEvent: mocks.trackAppEvent, trackNavigationClicked: mocks.trackNavigationClicked,
  identifyAnalyticsUser: mocks.identifyAnalyticsUser, resetAnalyticsIdentity: mocks.resetAnalyticsIdentity,
  hasStaleAnalyticsIdentity: mocks.hasStaleAnalyticsIdentity, setAnalyticsContext: mocks.setAnalyticsContext,
  captureProductEvent: mocks.captureProductEvent,
  initAnalytics: mocks.initAnalytics,
}));
vi.mock("@clerk/nextjs", () => ({
  SignOutButton: ({ children }: { children: React.ReactNode }) => children, UserProfile: () => null,
  UserButton: () => React.createElement("div"), useUser: mocks.useUser,
  ClerkProvider: ({ children }: { children: React.ReactNode }) => React.createElement(React.Fragment, null, children),
  SignIn: () => React.createElement("div"), SignUp: () => React.createElement("div"),
  useAuth: () => ({ getToken: async () => "token_stub" }),
}));
vi.mock("next/navigation", () => ({
  useSearchParams: () => new URLSearchParams(), usePathname: mocks.usePathname, useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
}));
vi.mock("next/link", () => ({
  default: ({ children, ...props }: React.PropsWithChildren<React.AnchorHTMLAttributes<HTMLAnchorElement>>) =>
    React.createElement("a", props, children),
}));

beforeEach(() => {
  mocks.useUser.mockReset();
  mocks.useUser.mockReturnValue({ user: null, isLoaded: true, isSignedIn: false });
  mocks.usePathname.mockReset();
  mocks.trackAppEvent.mockReset();
  mocks.trackNavigationClicked.mockReset();
  mocks.identifyAnalyticsUser.mockReset();
  mocks.resetAnalyticsIdentity.mockReset();
  mocks.hasStaleAnalyticsIdentity.mockReset();
  mocks.hasStaleAnalyticsIdentity.mockReturnValue(false);
  mocks.setAnalyticsContext.mockReset();
  mocks.captureProductEvent.mockReset();
  mocks.usePathname.mockReturnValue("/workspaces");
  window.history.replaceState(null, "", "/workspaces");
  mocks.initAnalytics.mockClear();
});

afterEach(() => {
  // Tear the rendered tree down between tests: a `render`-based test that fails
  // an assertion before its trailing `cleanup()` would otherwise leave a second
  // Shell in the DOM and turn every later `getByRole` into an ambiguous match.
  cleanup();
  vi.clearAllMocks();
});

describe("app components", () => {
  it("tracks shell navigation", async () => {
    const { ShellFrame: Shell } = await import("../components/layout/ShellFrame");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    const { container } = render(
      React.createElement(Shell, null, React.createElement("div", null, "content")),
    );

    // Clicking a sidebar nav link emits a navigation-analytics event.
    await user.click(screen.getByText("Your fleets"));
    expect(mocks.trackNavigationClicked).toHaveBeenCalled();

    // Brand-mark + wordmark are the topbar shape — Operational Restraint:
    // no decorative badges, no marketing chrome.
    expect(container.innerHTML).toContain("agentsfleet");
    expect(container.innerHTML).toContain("data-brand-mark");
    cleanup();
  });

  it("clears the workspace toast fade timer when the shell unmounts", async () => {
    const { ShellFrame: Shell } = await import("../components/layout/ShellFrame");
    const setTimeoutSpy = vi.spyOn(globalThis, "setTimeout");
    const clearTimeoutSpy = vi.spyOn(globalThis, "clearTimeout");
    try {
      const { unmount } = render(
        React.createElement(Shell, null, React.createElement("div", null, "content")),
      );
      const fadeTimerIndex = setTimeoutSpy.mock.calls.findIndex(
        ([, delay]) => delay === TOAST_FADE_MS,
      );

      expect(screen.getByTestId("workspace-toast")).toBeDefined();
      expect(fadeTimerIndex).toBeGreaterThanOrEqual(0);
      const fadeTimer = setTimeoutSpy.mock.results[fadeTimerIndex]!.value;
      unmount();
      expect(clearTimeoutSpy).toHaveBeenCalledWith(fadeTimer);
    } finally {
      clearTimeoutSpy.mockRestore();
      setTimeoutSpy.mockRestore();
    }
  });

  it("test_navigation_does_not_duplicate_live_subscriptions", async () => {
    const { ShellFrame: Shell } = await import("../components/layout/ShellFrame");
    // M118: Shell derives the active workspace from the route (`/w/<id>/…`), not
    // an `activeWorkspaceId` prop — the pathname is the single source of truth.
    mocks.usePathname.mockReturnValue("/w/ws_1");
    const { rerender } = render(
      React.createElement(
        Shell,
        {
          workspaces: [
            { id: "ws_1", name: "Alpha", created_at: "1970-01-01T00:00:00.001Z", ...OWN_ACCOUNT },
            { id: "ws_2", name: "Beta", created_at: "1970-01-01T00:00:00.002Z", ...OWN_ACCOUNT },
          ],
        } as never,
        React.createElement("div", null, "content"),
      ),
    );
    // The Shell effect binds the PostHog workspace group + records the count.
    expect(mocks.setAnalyticsContext).toHaveBeenCalledWith({ workspaceId: "ws_1", workspaceCount: 2 });
    rerender(
      React.createElement(
        Shell,
        {
          workspaces: [
            { id: "ws_1", name: "Alpha", created_at: "1970-01-01T00:00:00.001Z", ...OWN_ACCOUNT },
            { id: "ws_2", name: "Beta", created_at: "1970-01-01T00:00:00.002Z", ...OWN_ACCOUNT },
          ],
        } as never,
        React.createElement("div", null, "next route content"),
      ),
    );
    expect(mocks.setAnalyticsContext).toHaveBeenCalledTimes(1);
    cleanup();
  });

  it("identifies the current clerk user once loaded", async () => {
    mocks.useUser.mockReturnValue({
      isLoaded: true,
      isSignedIn: true,
      user: {
        id: "user_123",
        primaryEmailAddress: { emailAddress: "kishore@example.com" },
      },
    });

    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    const { container } = render(React.createElement(AnalyticsBootstrap));
    await act(async () => {});

    expect(container.innerHTML).toBe("");
    expect(mocks.identifyAnalyticsUser).toHaveBeenCalledWith({
      id: "user_123",
      email: "kishore@example.com",
    });
  });

  it("does not identify until clerk user data is ready", async () => {
    mocks.useUser.mockReturnValue({
      isLoaded: false,
      isSignedIn: false,
      user: null,
    });

    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    render(React.createElement(AnalyticsBootstrap));
    await act(async () => {});

    expect(mocks.identifyAnalyticsUser).not.toHaveBeenCalled();
    expect(mocks.resetAnalyticsIdentity).not.toHaveBeenCalled();
  });

  it("does nothing while signed in but the user object is still resolving", async () => {
    mocks.useUser.mockReturnValue({ isLoaded: true, isSignedIn: true, user: null });

    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    render(React.createElement(AnalyticsBootstrap));
    await act(async () => {});

    expect(mocks.identifyAnalyticsUser).not.toHaveBeenCalled();
    expect(mocks.resetAnalyticsIdentity).not.toHaveBeenCalled();
  });

  it("resets analytics identity once when signed out with a lingering identity", async () => {
    mocks.useUser.mockReturnValue({ isLoaded: true, isSignedIn: false, user: null });
    // The first signed-out render still carries the prior session's identity…
    mocks.hasStaleAnalyticsIdentity.mockReturnValueOnce(true);

    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    const { rerender } = render(React.createElement(AnalyticsBootstrap));
    // …and once reset clears it, repeated signed-out renders are no-ops.
    rerender(React.createElement(AnalyticsBootstrap));
    rerender(React.createElement(AnalyticsBootstrap));
    await act(async () => {});

    expect(mocks.resetAnalyticsIdentity).toHaveBeenCalledTimes(1);
    expect(mocks.identifyAnalyticsUser).not.toHaveBeenCalled();
  });

  it("never resets for an anonymous visitor with no prior identity", async () => {
    mocks.useUser.mockReturnValue({ isLoaded: true, isSignedIn: false, user: null });

    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    const { rerender } = render(React.createElement(AnalyticsBootstrap));
    rerender(React.createElement(AnalyticsBootstrap));
    await act(async () => {});

    expect(mocks.resetAnalyticsIdentity).not.toHaveBeenCalled();
  });

  it("sign-out edge resets, then a fresh sign-in identifies again", async () => {
    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    mocks.useUser.mockReturnValue({
      isLoaded: true,
      isSignedIn: true,
      user: { id: "user_123", primaryEmailAddress: { emailAddress: "kishore@example.com" } },
    });
    const { rerender } = render(React.createElement(AnalyticsBootstrap));
    await act(async () => {});
    expect(mocks.identifyAnalyticsUser).toHaveBeenCalledTimes(1);

    // Sign-out: the analytics module reports the identity as stale once.
    mocks.useUser.mockReturnValue({ isLoaded: true, isSignedIn: false, user: null });
    mocks.hasStaleAnalyticsIdentity.mockReturnValueOnce(true);
    rerender(React.createElement(AnalyticsBootstrap));
    await act(async () => {});
    expect(mocks.resetAnalyticsIdentity).toHaveBeenCalledTimes(1);

    // Re-login: identify fires again for the new session.
    mocks.useUser.mockReturnValue({
      isLoaded: true,
      isSignedIn: true,
      user: { id: "user_123", primaryEmailAddress: { emailAddress: "kishore@example.com" } },
    });
    rerender(React.createElement(AnalyticsBootstrap));
    await act(async () => {});
    expect(mocks.identifyAnalyticsUser).toHaveBeenCalledTimes(2);
  });


  it("renders Shell with the shared signal mark and sidebar nav", async () => {
    const { ShellFrame: Shell } = await import("../components/layout/ShellFrame");
    // M118: a workspace is in the route, so workspace-scoped nav hrefs carry the
    // `/w/<id>` segment; tenant/platform items (API Keys, Billing) stay at root.
    mocks.usePathname.mockReturnValue("/w/ws_1/fleets");
    const markup = renderToStaticMarkup(
      React.createElement(Shell, null, React.createElement("div", null, "content")),
    );
    expect(markup).toContain('data-brand-mark="signal"');
    expect(markup).toContain("agentsfleet");
    expect(markup).toContain("New fleet");
    expect(markup).toContain("Your fleets");
    expect(markup).toContain("Workspace and account menu");
    expect(markup).not.toContain('href="/w/ws_1/settings/models"');
  });

});
