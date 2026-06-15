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
  SkullIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "SkullIcon" }),
  SettingsIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "SettingsIcon" }),
  BookOpenIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "BookOpenIcon" }),
  ZapIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "ZapIcon" }),
  ShieldIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "ShieldIcon" }),
  KeyRoundIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "KeyRoundIcon" }),
  CheckCircle2Icon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "CheckCircle2Icon" }),
  ServerIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "ServerIcon" }),
  CpuIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "CpuIcon" }),
  CreditCardIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "CreditCardIcon" }),
  MenuIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "MenuIcon" }),
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

beforeEach(() => {
  mocks.useUser.mockReset();
  mocks.usePathname.mockReset();
  mocks.trackAppEvent.mockReset();
  mocks.trackNavigationClicked.mockReset();
  mocks.identifyAnalyticsUser.mockReset();
  mocks.resetAnalyticsIdentity.mockReset();
  mocks.hasStaleAnalyticsIdentity.mockReset();
  mocks.hasStaleAnalyticsIdentity.mockReturnValue(false);
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
    mocks.usePathname.mockReturnValue("/agents");
    const tree = Shell({ children: React.createElement("div", null, "content") });
    const markup = renderToStaticMarkup(React.createElement(React.Fragment, null, tree));
    // Brand-mark always-alive contract.
    expect(markup).toContain("data-live");
    expect(markup).toContain("agentsfleet");
    // Sidebar nav rendered across the Operations / Configuration / Organization
    // groups, plus the Dashboard overview entry and the Docs footer link.
    expect(markup).toContain("Operations");
    expect(markup).toContain("Configuration");
    expect(markup).toContain("Organization");
    expect(markup).toContain("Dashboard");
    expect(markup).toContain("Agents");
    // Models + Credentials are one combined Configuration entry now.
    expect(markup).toContain("Models &amp; Credentials");
    // The combined item points at the unified route.
    expect(markup).toContain('href="/settings/models"');
    // The standalone Credentials nav link is gone (it redirects into the page).
    expect(markup).not.toContain('href="/credentials"');
    expect(markup).toContain("Approvals");
    expect(markup).toContain("Events");
    expect(markup).toContain("Settings");
    expect(markup).toContain("Billing");
  });

  it("Shell sidebar marks the active route via data-active attribute", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/agents");
    const tree = Shell({ children: React.createElement("div") });
    const markup = renderToStaticMarkup(React.createElement(React.Fragment, null, tree));
    // The active link gets data-active="true" — the sidebar's surface-3 fill
    // is driven from this attribute (no coloured bar per spec).
    expect(markup).toMatch(/data-active="true"[^>]*>\s*<svg[^>]*data-icon="SkullIcon"/);
  });

  it("Shell active-link resolves the longest-matching prefix (Settings collision)", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    // Render at a pathname and report how many items are active + the active
    // item's icon — exactly one item must light, and it must be the most
    // specific match (a nested /settings/* child beats its parent Settings).
    const activeFor = (pathname: string, isPlatformAdmin = false) => {
      mocks.usePathname.mockReturnValue(pathname);
      const tree = Shell({ children: React.createElement("div"), isPlatformAdmin });
      const markup = renderToStaticMarkup(React.createElement(React.Fragment, null, tree));
      const count = (markup.match(/data-active="true"/g) ?? []).length;
      const icon = markup.match(/data-active="true"[^>]*>\s*<svg[^>]*data-icon="([^"]+)"/)?.[1] ?? null;
      return { count, icon };
    };
    // Nested children win over the parent Settings (the bug the resolver fixes).
    expect(activeFor("/settings/models")).toEqual({ count: 1, icon: "CpuIcon" });
    expect(activeFor("/settings/billing")).toEqual({ count: 1, icon: "CreditCardIcon" });
    // Parent Settings lights on its own route and on unclaimed children only.
    expect(activeFor("/settings")).toEqual({ count: 1, icon: "SettingsIcon" });
    expect(activeFor("/settings/api-keys")).toEqual({ count: 1, icon: "SettingsIcon" });
    // Other groups resolve to their own item; root and admin-gated paths too.
    expect(activeFor("/")).toEqual({ count: 1, icon: "LayoutDashboardIcon" });
    expect(activeFor("/admin/runners", true)).toEqual({ count: 1, icon: "ServerIcon" });
  });

  it("Shell appends the platform-admin Runners item only when isPlatformAdmin", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const tree = Shell({ children: React.createElement("div"), isPlatformAdmin: true });
    const markup = renderToStaticMarkup(React.createElement(React.Fragment, null, tree));
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

  it("Shell hides the platform-admin surface for a non-admin session", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    // Default (no isPlatformAdmin prop) → the Platform nav group is absent. This
    // is discoverability only; the backend independently gates the route.
    const tree = Shell({ children: React.createElement("div") });
    const markup = renderToStaticMarkup(React.createElement(React.Fragment, null, tree));
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
    expect(dialog.textContent).toContain("Agents");
    cleanup();
  });

  it("Shell mobile-nav: clicking a link inside the dialog closes it", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/agents");
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
    // 'Agents' (href /agents) exercises the path-to-slug replaceAll branch.
    await user.click(screen.getByText("Dashboard"));
    await user.click(screen.getByText("Agents"));
    // Footer 'Docs' is external (anchor + label-slug branch); 'Settings' internal.
    await user.click(screen.getByText("Docs"));
    await user.click(screen.getByText("Settings"));
    // The combined Configuration entry — a nested route exercises the
    // multi-segment slug branch.
    await user.click(screen.getByText("Models & Credentials"));
    await user.click(screen.getByText("Billing"));

    const sources = mocks.trackNavigationClicked.mock.calls.map(
      (c) => (c[0] as { source: string }).source,
    );
    expect(sources).toContain("app_sidebar_root");
    expect(sources).toContain("app_sidebar_agents");
    expect(sources).toContain("app_sidebar_docs");
    expect(sources).toContain("app_sidebar_settings");
    expect(sources).toContain("app_sidebar_settings_models");
    expect(sources).toContain("app_sidebar_settings_billing");
    cleanup();
  });
});
