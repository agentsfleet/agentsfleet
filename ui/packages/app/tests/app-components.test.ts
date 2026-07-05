import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { render, screen, cleanup, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const mocks = vi.hoisted(() => ({
  trackAppEvent: vi.fn(),
  trackNavigationClicked: vi.fn(),
  identifyAnalyticsUser: vi.fn(),
  resetAnalyticsIdentity: vi.fn(),
  hasStaleAnalyticsIdentity: vi.fn(() => false),
  setAnalyticsContext: vi.fn(),
  captureProductEvent: vi.fn(),
  useUser: vi.fn(),
  usePathname: vi.fn(),
  useEffectMock: vi.fn((fn: () => void) => fn()),
}));

vi.mock("react", async () => {
  const actual = await vi.importActual<typeof import("react")>("react");
  return { ...actual, useEffect: mocks.useEffectMock };
});

vi.mock("@/lib/analytics/posthog", () => ({
  trackAppEvent: mocks.trackAppEvent,
  trackNavigationClicked: mocks.trackNavigationClicked,
  identifyAnalyticsUser: mocks.identifyAnalyticsUser,
  resetAnalyticsIdentity: mocks.resetAnalyticsIdentity,
  hasStaleAnalyticsIdentity: mocks.hasStaleAnalyticsIdentity,
  setAnalyticsContext: mocks.setAnalyticsContext,
  captureProductEvent: mocks.captureProductEvent,
}));

vi.mock("@clerk/nextjs", () => ({
  UserButton: () => React.createElement("div", { "data-user-button": "1" }),
  useUser: mocks.useUser,
  ClerkProvider: ({ children }: { children: React.ReactNode }) => React.createElement(React.Fragment, null, children),
  SignIn: () => React.createElement("div", { "data-sign-in": "1" }),
  SignUp: () => React.createElement("div", { "data-sign-up": "1" }),
  useAuth: () => ({ getToken: async () => "token_stub" }),
}));

vi.mock("next/navigation", () => ({
  usePathname: mocks.usePathname,
  useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
}));

vi.mock("next/link", () => ({
  default: ({ children, ...props }: React.PropsWithChildren<React.AnchorHTMLAttributes<HTMLAnchorElement>>) =>
    React.createElement("a", props, children),
}));

vi.mock("lucide-react", () => ({
  GitBranchIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "GitBranchIcon" }),
  ActivityIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "ActivityIcon" }),
  PauseIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "PauseIcon" }),
  ExternalLinkIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "ExternalLinkIcon" }),
  LayoutDashboardIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "LayoutDashboardIcon" }),
  BoxIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "BoxIcon" }),
  BotIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "BotIcon" }),
  SettingsIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "SettingsIcon" }),
  KeyIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "KeyIcon" }),
  BookOpenIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "BookOpenIcon" }),
  ZapIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "ZapIcon" }),
  ShieldIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "ShieldIcon" }),
  KeyRoundIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "KeyRoundIcon" }),
  LinkIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "LinkIcon" }),
  CheckCircle2Icon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "CheckCircle2Icon" }),
  ServerIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "ServerIcon" }),
  CpuIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "CpuIcon" }),
  CreditCardIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "CreditCardIcon" }),
  MenuIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "MenuIcon" }),
  PanelLeftIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "PanelLeftIcon" }),
  SunIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "SunIcon" }),
  MoonIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "MoonIcon" }),
  ChevronDownIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "ChevronDownIcon" }),
  PlusIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "PlusIcon" }),
}));

// ThemeToggle setStates inside a useEffect; this file's synchronous useEffect
// mock (runs every render, ignores deps) would loop on it. These tests cover
// Shell nav, not theming — stub it.
vi.mock("@/components/layout/ThemeToggle", () => ({
  default: () => React.createElement("button", { "data-theme-toggle": "1" }),
}));

vi.mock("@/components/layout/ClientOnlyAuthUserButton", () => ({
  default: () => React.createElement("div", { "data-user-button": "1" }),
}));

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
  mocks.useEffectMock.mockClear();
  mocks.usePathname.mockReturnValue("/workspaces");
});

afterEach(() => {
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
    await user.click(screen.getByText("Dashboard"));
    expect(mocks.trackNavigationClicked).toHaveBeenCalled();

    // Brand-mark + wordmark are the topbar shape — Operational Restraint:
    // no decorative badges, no marketing chrome.
    expect(container.innerHTML).toContain("agentsfleet");
    expect(container.innerHTML).toContain("data-live");
    cleanup();
  });

  it("binds the analytics workspace context from the active workspace + count", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    render(
      React.createElement(
        Shell,
        {
          workspaces: [
            { id: "ws_1", name: "Alpha", created_at: 1 },
            { id: "ws_2", name: "Beta", created_at: 2 },
          ],
          activeWorkspaceId: "ws_1",
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

    const tree = AnalyticsBootstrap();

    expect(tree).toBeNull();
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

    AnalyticsBootstrap();

    expect(mocks.identifyAnalyticsUser).not.toHaveBeenCalled();
    expect(mocks.resetAnalyticsIdentity).not.toHaveBeenCalled();
  });

  it("does nothing while signed in but the user object is still resolving", async () => {
    mocks.useUser.mockReturnValue({ isLoaded: true, isSignedIn: true, user: null });

    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    AnalyticsBootstrap();

    expect(mocks.identifyAnalyticsUser).not.toHaveBeenCalled();
    expect(mocks.resetAnalyticsIdentity).not.toHaveBeenCalled();
  });

  it("resets analytics identity once when signed out with a lingering identity", async () => {
    mocks.useUser.mockReturnValue({ isLoaded: true, isSignedIn: false, user: null });
    // The first signed-out render still carries the prior session's identity…
    mocks.hasStaleAnalyticsIdentity.mockReturnValueOnce(true);

    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    AnalyticsBootstrap();
    // …and once reset clears it, repeated signed-out renders are no-ops.
    AnalyticsBootstrap();
    AnalyticsBootstrap();

    expect(mocks.resetAnalyticsIdentity).toHaveBeenCalledTimes(1);
    expect(mocks.identifyAnalyticsUser).not.toHaveBeenCalled();
  });

  it("never resets for an anonymous visitor with no prior identity", async () => {
    mocks.useUser.mockReturnValue({ isLoaded: true, isSignedIn: false, user: null });

    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    AnalyticsBootstrap();
    AnalyticsBootstrap();

    expect(mocks.resetAnalyticsIdentity).not.toHaveBeenCalled();
  });

  it("sign-out edge resets, then a fresh sign-in identifies again", async () => {
    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    mocks.useUser.mockReturnValue({
      isLoaded: true,
      isSignedIn: true,
      user: { id: "user_123", primaryEmailAddress: { emailAddress: "kishore@example.com" } },
    });
    AnalyticsBootstrap();
    expect(mocks.identifyAnalyticsUser).toHaveBeenCalledTimes(1);

    // Sign-out: the analytics module reports the identity as stale once.
    mocks.useUser.mockReturnValue({ isLoaded: true, isSignedIn: false, user: null });
    mocks.hasStaleAnalyticsIdentity.mockReturnValueOnce(true);
    AnalyticsBootstrap();
    expect(mocks.resetAnalyticsIdentity).toHaveBeenCalledTimes(1);

    // Re-login: identify fires again for the new session.
    mocks.useUser.mockReturnValue({
      isLoaded: true,
      isSignedIn: true,
      user: { id: "user_123", primaryEmailAddress: { emailAddress: "kishore@example.com" } },
    });
    AnalyticsBootstrap();
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
    mocks.usePathname.mockReturnValue("/fleets");
    const markup = renderToStaticMarkup(
      React.createElement(Shell, null, React.createElement("div", null, "content")),
    );
    expect(markup).toContain("data-live");
    expect(markup).toContain("agentsfleet");
    // Sidebar nav rendered across the Automations / Configuration / Organization
    // groups, plus the Dashboard overview entry and the Docs footer link.
    expect(markup).toContain("Automations");
    expect(markup).toContain("Configuration");
    expect(markup).toContain("Organization");
    expect(markup).toContain("Dashboard");
    expect(markup).toContain("Fleets");
    // Models, Integrations, and Secrets & ENVs are the three Configuration
    // entries — Secrets & ENVs is standalone again, not folded into Models.
    expect(markup).toContain(">Models<");
    expect(markup).toContain(">Integrations<");
    expect(markup).toContain(">Secrets &amp; ENVs<");
    expect(markup).toContain('href="/settings/models"');
    // Integrations is its own connectors destination.
    expect(markup).toContain('href="/integrations"');
    // Secrets & ENVs is its own standalone destination.
    expect(markup).toContain('href="/secrets"');
    expect(markup).toContain("Approvals");
    expect(markup).toContain("Events");
    expect(markup).toContain("API Keys");
    expect(markup).toContain("Billing");
  });

  it("test_nav_config_destinations: nav renders Models→/settings/models, Integrations→/integrations, Secrets & ENVs→/secrets", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const markup = renderToStaticMarkup(
      React.createElement(Shell, null, React.createElement("div", null, "content")),
    );
    // Three distinct Configuration destinations, each at its own route.
    expect(markup).toMatch(/href="\/settings\/models"[\s\S]*?data-icon="CpuIcon"[^>]*><\/svg>Models</);
    expect(markup).toMatch(/href="\/integrations"[\s\S]*?data-icon="LinkIcon"[^>]*><\/svg>Integrations</);
    expect(markup).toMatch(/href="\/secrets"[\s\S]*?data-icon="KeyRoundIcon"[^>]*><\/svg>Secrets &amp; ENVs</);
  });

  it("Shell sidebar marks the active route via data-active attribute", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/fleets");
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
    const activeFor = (pathname: string, operatorScopes: string[] = []) => {
      mocks.usePathname.mockReturnValue(pathname);
      const markup = renderToStaticMarkup(
        // createElement's props-plus-rest-children overload can't see that the
        // third arg below satisfies Shell's required `children` prop (a known
        // @types/react gap when children is non-optional) — asserting the
        // props object's shape is safe since Shell reads children from React's
        // normal children slot regardless of how it arrived.
        React.createElement(Shell, { operatorScopes } as React.ComponentProps<typeof Shell>, React.createElement("div")),
      );
      const count = (markup.match(/data-active="true"/g) ?? []).length;
      const icon = markup.match(/data-active="true"[^>]*>\s*<svg[^>]*data-icon="([^"]+)"/)?.[1] ?? null;
      return { count, icon };
    };
    expect(activeFor("/settings/models")).toEqual({ count: 1, icon: "CpuIcon" });
    expect(activeFor("/settings/billing")).toEqual({ count: 1, icon: "CreditCardIcon" });
    // API Keys now owns its own distinct href — nested children (mint/reveal
    // detail routes, if any) still resolve to it via prefix match; a nested
    // child under Models must not spuriously light up API Keys instead.
    expect(activeFor("/settings/api-keys")).toEqual({ count: 1, icon: "KeyIcon" });
    // /settings on its own has no nav entry (it's a redirect-only route, the
    // Workspace tab folded into API Keys) — nothing lights up.
    expect(activeFor("/settings")).toEqual({ count: 0, icon: null });
    // Other groups resolve to their own item; root and admin-gated paths too.
    expect(activeFor("/")).toEqual({ count: 1, icon: "LayoutDashboardIcon" });
    expect(activeFor("/admin/runners", ["runner:read"])).toEqual({ count: 1, icon: "ServerIcon" });
  });

  it("Shell appends the Runners item only when the session holds runner:read", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const markup = renderToStaticMarkup(
      React.createElement(
        Shell,
        { operatorScopes: ["runner:read"] } as React.ComponentProps<typeof Shell>,
        React.createElement("div"),
      ),
    );
    // Runners joins the Configuration group with its link + ServerIcon glyph.
    expect(markup).toContain("Configuration");
    expect(markup).toContain("Runners");
    expect(markup).toContain('href="/admin/runners"');
    expect(markup).toContain('data-icon="ServerIcon"');
    // It is appended to Configuration, not rendered as a separate group: the
    // Configuration header appears exactly once and there is no "Platform" group.
    expect((markup.match(/>Configuration</g) ?? []).length).toBe(1);
    expect(markup).not.toMatch(/>\s*Platform\s*</);
  });

  it("Shell hides the platform surface for a session without operator scopes", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    // Default (no operatorScopes prop) → the platform nav items are absent. This
    // is discoverability only; the backend independently gates the route.
    const markup = renderToStaticMarkup(React.createElement(Shell, null, React.createElement("div")));
    expect(markup).not.toContain('href="/admin/runners"');
    expect(markup).not.toContain('data-icon="ServerIcon"');
  });

  it("Shell mobile-nav: hamburger button is present (md:hidden)", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    render(
      React.createElement(Shell, null, React.createElement("div", null, "content")),
    );
    // The mobile hamburger renders as a Button with aria-label="Open navigation".
    // It exists in the DOM at all viewports; CSS hides it ≥md.
    const hamburger = screen.getByRole("button", { name: /open navigation/i });
    expect(hamburger).toBeTruthy();
    expect(hamburger.className).toContain("md:hidden");
    cleanup();
  });

  it("Shell mobile-nav: clicking hamburger opens the dialog with sidebar nav", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    render(
      React.createElement(Shell, null, React.createElement("div", null, "content")),
    );
    const hamburger = screen.getByRole("button", { name: /open navigation/i });
    await user.click(hamburger);
    // Dialog renders the SidebarNav which carries the same 5 operational
    // links. The dialog itself is keyed by an accessible "Navigation" title.
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toBeTruthy();
    expect(dialog.textContent).toContain("Dashboard");
    expect(dialog.textContent).toContain("Fleets");
    cleanup();
  });

  it("Shell mobile-nav: clicking a link inside the dialog closes it", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/fleets");
    const user = userEvent.setup();
    render(
      React.createElement(Shell, null, React.createElement("div", null, "content")),
    );
    await user.click(screen.getByRole("button", { name: /open navigation/i }));
    const dialog = await screen.findByRole("dialog");
    // Clicking a nav link fires the dialog instance's onNavigate (setOpen(false)),
    // collapsing the mobile sheet — the desktop sidebar passes a no-op instead.
    await user.click(within(dialog).getByRole("link", { name: /dashboard/i }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    cleanup();
  });

  it("emits navigation analytics from sidebar and bottom-nav links", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    render(
      React.createElement(Shell, null, React.createElement("div", null, "content")),
    );
    // Sidebar 'Dashboard' is href "/" → the source uses the 'root' branch;
    // 'Fleets' (href /fleets) exercises the path-to-slug replaceAll branch.
    await user.click(screen.getByText("Dashboard"));
    await user.click(screen.getByText("Fleets"));
    // Footer 'Docs' is external; 'API Keys' is internal.
    await user.click(screen.getByText("Docs"));
    await user.click(screen.getByText("API Keys"));
    // The Models Configuration entry — a nested route exercises the
    // multi-segment slug branch.
    await user.click(screen.getByText("Models"));
    await user.click(screen.getByText("Billing"));

    const sources = mocks.trackNavigationClicked.mock.calls.map(
      (c) => (c[0] as { source: string }).source,
    );
    expect(sources).toContain("app_sidebar_root");
    expect(sources).toContain("app_sidebar_fleets");
    expect(sources).toContain("app_sidebar_docs");
    expect(sources).toContain("app_sidebar_settings_api-keys");
    expect(sources).toContain("app_sidebar_settings_models");
    expect(sources).toContain("app_sidebar_settings_billing");
    cleanup();
  });

  it("should collapse the sidebar and hide nav labels when the toggle is clicked", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));

    const toggle = screen.getByRole("button", { name: /collapse sidebar/i });
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    // Label text is visible pre-collapse — plain getByText, not getByRole,
    // since NavItem renders the label as a text node beside the icon.
    expect(screen.getByText("Dashboard")).toBeTruthy();

    await user.click(toggle);

    // aria-expanded flips and the label swaps to "Expand sidebar" — proves
    // the button reflects state, not just that a click handler ran.
    expect(screen.getByRole("button", { name: /expand sidebar/i })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /collapse sidebar/i })).toBeNull();
    expect(screen.getByRole("button", { name: /expand sidebar/i }).getAttribute("aria-expanded")).toBe(
      "false",
    );
    // The label text node is gone — collapsed renders icon-only.
    expect(screen.queryByText("Dashboard")).toBeNull();
    cleanup();
  });

  it("should expand the sidebar again when the toggle is clicked twice", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));

    const toggle = screen.getByRole("button", { name: /collapse sidebar/i });
    await user.click(toggle);
    expect(screen.queryByText("Dashboard")).toBeNull();

    await user.click(screen.getByRole("button", { name: /expand sidebar/i }));
    // Round-trip: labels are back, and the button reverts to "Collapse sidebar".
    expect(screen.getByText("Dashboard")).toBeTruthy();
    expect(screen.getByRole("button", { name: /collapse sidebar/i })).toBeTruthy();
    cleanup();
  });

  it("should keep nav links accessible by name when collapsed (icon-only, no visible text)", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));

    await user.click(screen.getByRole("button", { name: /collapse sidebar/i }));

    // No visible label text, but the link is still reachable by its accessible
    // name (title/aria-label) — a screen-reader user isn't stranded by the
    // icon-only rail. getByRole with `name` matches the accessible name
    // computation, not textContent, so this fails if aria-label is dropped.
    const fleetsLink = screen.getByRole("link", { name: "Fleets" });
    expect(fleetsLink).toBeTruthy();
    expect(fleetsLink.getAttribute("href")).toBe("/fleets");
    cleanup();
  });

  it("should hide section group labels (Automations/Configuration/Organization) when collapsed", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));

    expect(screen.getByText("Automations")).toBeTruthy();
    expect(screen.getByText("Configuration")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: /collapse sidebar/i }));

    expect(screen.queryByText("Automations")).toBeNull();
    expect(screen.queryByText("Configuration")).toBeNull();
    expect(screen.queryByText("Organization")).toBeNull();
    cleanup();
  });

  it("should render the active nav link with the mint/pulse styling classes, not the generic accent", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/fleets");
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));

    const activeLink = screen.getByRole("link", { name: "Fleets" });
    expect(activeLink.getAttribute("data-active")).toBe("true");
    expect(activeLink.className).toContain("data-[active=true]:bg-pulse/10");
    expect(activeLink.className).toContain("data-[active=true]:text-pulse");
    // Regression guard: the old generic-accent active styling must not
    // resurface — a revert to `bg-accent` for the active state would pass a
    // sloppier assertion that only checks presence of *a* highlight class.
    expect(activeLink.className).not.toContain("data-[active=true]:bg-accent");
    cleanup();
  });

  it("should render a left accent bar on the active nav item, on top of the fill", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/fleets");
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));

    const activeLink = screen.getByRole("link", { name: "Fleets" });
    expect(activeLink.className).toContain("border-l-2");
    expect(activeLink.className).toContain("border-transparent");
    expect(activeLink.className).toContain("data-[active=true]:border-pulse");

    const inactiveLink = screen.getByRole("link", { name: "Events" });
    expect(inactiveLink.getAttribute("data-active")).toBeNull();
    expect(inactiveLink.className).toContain("border-l-2");
    cleanup();
  });

  it("mobile nav dialog always renders expanded, regardless of the desktop collapse state", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));

    // Collapse the desktop sidebar first.
    await user.click(screen.getByRole("button", { name: /collapse sidebar/i }));
    expect(screen.queryByText("Dashboard")).toBeNull();

    // The mobile dialog is a structurally separate SidebarNav instance
    // hardcoded to collapsed={false} — opening it must show full labels even
    // though the desktop instance is currently collapsed. This pins that the
    // two instances never accidentally share the collapsed prop.
    await user.click(screen.getByRole("button", { name: /open navigation/i }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Dashboard")).toBeTruthy();
    expect(within(dialog).getByText("Fleets")).toBeTruthy();
    cleanup();
  });
});
