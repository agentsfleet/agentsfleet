import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { render, screen, cleanup, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
const TOAST_FADE_MS = 240;
const mocks = vi.hoisted(() => ({
  trackAppEvent: vi.fn(), trackNavigationClicked: vi.fn(),
  identifyAnalyticsUser: vi.fn(), resetAnalyticsIdentity: vi.fn(),
  hasStaleAnalyticsIdentity: vi.fn(() => false), setAnalyticsContext: vi.fn(),
  captureProductEvent: vi.fn(), useUser: vi.fn(), usePathname: vi.fn(),
}));

vi.mock("@/lib/analytics/posthog", () => ({
  trackAppEvent: mocks.trackAppEvent, trackNavigationClicked: mocks.trackNavigationClicked,
  identifyAnalyticsUser: mocks.identifyAnalyticsUser, resetAnalyticsIdentity: mocks.resetAnalyticsIdentity,
  hasStaleAnalyticsIdentity: mocks.hasStaleAnalyticsIdentity, setAnalyticsContext: mocks.setAnalyticsContext,
  captureProductEvent: mocks.captureProductEvent,
}));
vi.mock("@clerk/nextjs", () => ({
  UserButton: () => React.createElement("div"), useUser: mocks.useUser,
  ClerkProvider: ({ children }: { children: React.ReactNode }) => React.createElement(React.Fragment, null, children),
  SignIn: () => React.createElement("div"), SignUp: () => React.createElement("div"),
  useAuth: () => ({ getToken: async () => "token_stub" }),
}));
vi.mock("next/navigation", () => ({
  usePathname: mocks.usePathname, useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
}));
vi.mock("next/link", () => ({
  default: ({ children, ...props }: React.PropsWithChildren<React.AnchorHTMLAttributes<HTMLAnchorElement>>) =>
    React.createElement("a", props, children),
}));
vi.mock("lucide-react", () => {
  const icon = (name: string) => (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": name });
  return {
    GitBranchIcon: icon("GitBranchIcon"), ActivityIcon: icon("ActivityIcon"), PauseIcon: icon("PauseIcon"), ExternalLinkIcon: icon("ExternalLinkIcon"),
    LayoutDashboardIcon: icon("LayoutDashboardIcon"), BoxIcon: icon("BoxIcon"), BotIcon: icon("BotIcon"), SettingsIcon: icon("SettingsIcon"),
    KeyIcon: icon("KeyIcon"), BookOpenIcon: icon("BookOpenIcon"), ZapIcon: icon("ZapIcon"), ShieldIcon: icon("ShieldIcon"),
    KeyRoundIcon: icon("KeyRoundIcon"), LibraryIcon: icon("LibraryIcon"), LinkIcon: icon("LinkIcon"), CheckCircle2Icon: icon("CheckCircle2Icon"), ServerIcon: icon("ServerIcon"),
    CpuIcon: icon("CpuIcon"), CoinsIcon: icon("CoinsIcon"), CreditCardIcon: icon("CreditCardIcon"), MenuIcon: icon("MenuIcon"),
    PanelLeftIcon: icon("PanelLeftIcon"), SunIcon: icon("SunIcon"), MoonIcon: icon("MoonIcon"), ChevronDownIcon: icon("ChevronDownIcon"), PlusIcon: icon("PlusIcon"),
  };
});
vi.mock("@/components/layout/ThemeToggle", () => ({ default: () => React.createElement("button") }));
vi.mock("@/components/layout/ClientOnlyAuthUserButton", () => ({ default: () => React.createElement("div") }));

beforeEach(() => {
  mocks.useUser.mockReset();
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
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    const { container } = render(
      React.createElement(Shell, null, React.createElement("div", null, "content")),
    );

    // Clicking a sidebar nav link emits a navigation-analytics event.
    await user.click(screen.getByText("Fleets"));
    expect(mocks.trackNavigationClicked).toHaveBeenCalled();

    // Brand-mark + wordmark are the topbar shape — Operational Restraint:
    // no decorative badges, no marketing chrome.
    expect(container.innerHTML).toContain("agentsfleet");
    expect(container.innerHTML).toContain("data-live");
    cleanup();
  });

  it("clears the workspace toast fade timer when the shell unmounts", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
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

  it("binds the analytics workspace context from the active workspace + count", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    // M118: Shell derives the active workspace from the route (`/w/<id>/…`), not
    // an `activeWorkspaceId` prop — the pathname is the single source of truth.
    mocks.usePathname.mockReturnValue("/w/ws_1");
    render(
      React.createElement(
        Shell,
        {
          workspaces: [
            { id: "ws_1", name: "Alpha", created_at: 1 },
            { id: "ws_2", name: "Beta", created_at: 2 },
          ],
        } as never,
        React.createElement("div", null, "content"),
      ),
    );
    // The Shell effect binds the PostHog workspace group + records the count.
    expect(mocks.setAnalyticsContext).toHaveBeenCalledWith({ workspaceId: "ws_1", workspaceCount: 2 });
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

    expect(mocks.identifyAnalyticsUser).not.toHaveBeenCalled();
    expect(mocks.resetAnalyticsIdentity).not.toHaveBeenCalled();
  });

  it("does nothing while signed in but the user object is still resolving", async () => {
    mocks.useUser.mockReturnValue({ isLoaded: true, isSignedIn: true, user: null });

    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    render(React.createElement(AnalyticsBootstrap));

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

    expect(mocks.resetAnalyticsIdentity).toHaveBeenCalledTimes(1);
    expect(mocks.identifyAnalyticsUser).not.toHaveBeenCalled();
  });

  it("never resets for an anonymous visitor with no prior identity", async () => {
    mocks.useUser.mockReturnValue({ isLoaded: true, isSignedIn: false, user: null });

    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    const { rerender } = render(React.createElement(AnalyticsBootstrap));
    rerender(React.createElement(AnalyticsBootstrap));

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
    expect(mocks.identifyAnalyticsUser).toHaveBeenCalledTimes(1);

    // Sign-out: the analytics module reports the identity as stale once.
    mocks.useUser.mockReturnValue({ isLoaded: true, isSignedIn: false, user: null });
    mocks.hasStaleAnalyticsIdentity.mockReturnValueOnce(true);
    rerender(React.createElement(AnalyticsBootstrap));
    expect(mocks.resetAnalyticsIdentity).toHaveBeenCalledTimes(1);

    // Re-login: identify fires again for the new session.
    mocks.useUser.mockReturnValue({
      isLoaded: true,
      isSignedIn: true,
      user: { id: "user_123", primaryEmailAddress: { emailAddress: "kishore@example.com" } },
    });
    rerender(React.createElement(AnalyticsBootstrap));
    expect(mocks.identifyAnalyticsUser).toHaveBeenCalledTimes(2);
  });

  it("exports stable auth appearance tokens", async () => {
    const { AUTH_APPEARANCE } = await import("../lib/clerkAppearance");

    // Clerk's primary CTA is the live signal — colorPrimary maps to --pulse;
    // foreground sits on near-black --bg for contrast. Footer flat surface-1
    // over a top border (spec forbids decorative gradients on chrome). Footer
    // links and identity-edit affordances are muted text, NOT --pulse — the
    // currency rule reserves --pulse for the primary CTA only.
    expect(AUTH_APPEARANCE.variables.colorPrimary).toBe("var(--pulse)");
    expect(AUTH_APPEARANCE.elements.formButtonPrimary.color).toBe("var(--bg)");
    expect(AUTH_APPEARANCE.elements.formButtonPrimary.backgroundColor).toBe("var(--pulse)");
    expect(AUTH_APPEARANCE.elements.footer.backgroundColor).toBe("var(--surface-1)");
    expect(AUTH_APPEARANCE.elements.footer).not.toHaveProperty("background");
    // Footer / link affordances stay muted (currency-rule guard).
    expect(AUTH_APPEARANCE.elements.footerActionLink.color).not.toBe("var(--pulse)");
    expect(AUTH_APPEARANCE.elements.identityPreviewEditButton.color).not.toBe("var(--pulse)");
    expect(AUTH_APPEARANCE.elements.formResendCodeLink.color).not.toBe("var(--pulse)");
  });

  it("renders Shell with brand-mark wake-pulse + sidebar nav", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    // M118: a workspace is in the route, so workspace-scoped nav hrefs carry the
    // `/w/<id>` segment; tenant/platform items (API Keys, Billing) stay at root.
    mocks.usePathname.mockReturnValue("/w/ws_1/fleets");
    const markup = renderToStaticMarkup(
      React.createElement(Shell, null, React.createElement("div", null, "content")),
    );
    expect(markup).toContain("data-live");
    expect(markup).toContain("agentsfleet");
    // Sidebar nav rendered across the Automations / Configuration / Organization
    // groups, plus the Docs footer link. The Wall (Fleets) leads — there is no
    // dashboard entry (single-route refactor).
    expect(markup).toContain("Automations");
    expect(markup).toContain("Configuration");
    expect(markup).toContain("Organization");
    expect(markup).not.toContain("Dashboard");
    expect(markup).toContain("Fleets");
    // Models, Integrations, and Secrets are the three Configuration
    // entries — Secrets is standalone again, not folded into Models.
    expect(markup).toContain(">Models<");
    expect(markup).toContain(">Integrations<");
    expect(markup).toContain(">Secrets<");
    expect(markup).toContain('href="/w/ws_1/settings/models"');
    // Integrations is its own connectors destination.
    expect(markup).toContain('href="/w/ws_1/integrations"');
    // Secrets is its own standalone destination.
    expect(markup).toContain('href="/w/ws_1/secrets"');
    expect(markup).toContain("Approvals");
    expect(markup).toContain("Events");
    expect(markup).toContain("API Keys");
    expect(markup).toContain("Billing");
  });

  it("test_nav_config_destinations: nav renders Models→/w/<id>/settings/models, Integrations→/w/<id>/integrations, Secrets→/w/<id>/secrets", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    // M118: the workspace in the route prefixes every Configuration destination.
    mocks.usePathname.mockReturnValue("/w/ws_1");
    const markup = renderToStaticMarkup(
      React.createElement(Shell, null, React.createElement("div", null, "content")),
    );
    // Three distinct Configuration destinations, each at its own workspace route.
    expect(markup).toMatch(/href="\/w\/ws_1\/settings\/models"[\s\S]*?data-icon="CpuIcon"[^>]*><\/svg>Models</);
    expect(markup).toMatch(/href="\/w\/ws_1\/integrations"[\s\S]*?data-icon="LinkIcon"[^>]*><\/svg>Integrations</);
    expect(markup).toMatch(/href="\/w\/ws_1\/secrets"[\s\S]*?data-icon="KeyRoundIcon"[^>]*><\/svg>Secrets</);
  });

  it("Shell sidebar marks the active route via data-active attribute", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/w/ws_1/fleets");
    const markup = renderToStaticMarkup(React.createElement(Shell, null, React.createElement("div")));
    // The active link gets data-active="true" — the sidebar's surface-3 fill
    // and the left accent bar are both driven from this attribute.
    expect(markup).toMatch(/data-active="true"[^>]*>\s*<svg[^>]*data-icon="BotIcon"/);
  });

  it("Shell active-link resolves the longest-matching prefix (nested /settings/* routes)", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    // Render at a pathname and report how many items are active + the active
    // item's icon — exactly one item must light, and it must be the most
    // specific match.
    // M118: seed one owned workspace so the Shell resolves workspace-scoped nav
    // hrefs (its `linkWorkspaceId` falls back to `workspaces[0]` on tenant pages);
    // workspace routes now carry the `/w/<id>` prefix, tenant/platform stay root.
    const activeFor = (pathname: string, operatorScopes: string[] = []) => {
      mocks.usePathname.mockReturnValue(pathname);
      const markup = renderToStaticMarkup(
        // createElement's props-plus-rest-children overload can't see that the
        // third arg below satisfies Shell's required `children` prop (a known
        // @types/react gap when children is non-optional) — asserting the
        // props object's shape is safe since Shell reads children from React's
        // normal children slot regardless of how it arrived.
        React.createElement(
          Shell,
          { operatorScopes, workspaces: [{ id: "ws_1", name: "Alpha", created_at: 1 }] } as React.ComponentProps<typeof Shell>,
          React.createElement("div"),
        ),
      );
      const count = (markup.match(/data-active="true"/g) ?? []).length;
      const icon = markup.match(/data-active="true"[^>]*>\s*<svg[^>]*data-icon="([^"]+)"/)?.[1] ?? null;
      return { count, icon };
    };
    expect(activeFor("/w/ws_1/settings/models")).toEqual({ count: 1, icon: "CpuIcon" });
    expect(activeFor("/settings/billing")).toEqual({ count: 1, icon: "CreditCardIcon" });
    // API Keys now owns its own distinct href — nested children (mint/reveal
    // detail routes, if any) still resolve to it via prefix match; a nested
    // child under Models must not spuriously light up API Keys instead.
    expect(activeFor("/settings/api-keys")).toEqual({ count: 1, icon: "KeyIcon" });
    // /settings on its own has no nav entry (it's a redirect-only route, the
    // Workspace tab folded into API Keys) — nothing lights up.
    expect(activeFor("/settings")).toEqual({ count: 0, icon: null });
    // The workspace home (`/w/<id>`) is a redirect-only route with no nav entry
    // — nothing lights up there. The Wall (`/w/<id>/fleets`) lights Fleets; other
    // groups resolve to their own item; root admin-gated paths too.
    expect(activeFor("/w/ws_1")).toEqual({ count: 0, icon: null });
    expect(activeFor("/w/ws_1/fleets")).toEqual({ count: 1, icon: "BotIcon" });
    expect(activeFor("/admin/runners", ["runner:read"])).toEqual({ count: 1, icon: "ServerIcon" });
  });

});
