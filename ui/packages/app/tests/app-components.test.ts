import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const mocks = vi.hoisted(() => ({
  trackAppEvent: vi.fn(),
  trackNavigationClicked: vi.fn(),
  identifyAnalyticsUser: vi.fn(),
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
  MenuIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "MenuIcon" }),
  ChevronDownIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "ChevronDownIcon" }),
  PlusIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "PlusIcon" }),
}));

type ClickableElement = React.ReactElement<{ children?: React.ReactNode; onClick?: (...args: unknown[]) => unknown }>;

function findElements(node: React.ReactNode, matcher: (element: ClickableElement) => boolean): ClickableElement[] {
  const results: ClickableElement[] = [];
  function walk(value: React.ReactNode) {
    if (!React.isValidElement(value)) return;
    const element = value as ClickableElement;
    if (matcher(element)) results.push(element);
    const children = element.props?.children;
    if (Array.isArray(children)) {
      children.forEach(walk);
      return;
    }
    if (children) walk(children);
  }
  walk(node);
  return results;
}

beforeEach(() => {
  mocks.useUser.mockReset();
  mocks.usePathname.mockReset();
  mocks.trackAppEvent.mockReset();
  mocks.trackNavigationClicked.mockReset();
  mocks.identifyAnalyticsUser.mockReset();
  mocks.useEffectMock.mockClear();
  mocks.usePathname.mockReturnValue("/workspaces");
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("app components", () => {
  it("tracks shell navigation", async () => {
    const { default: Shell } = await import("../components/layout/Shell");

    const shellTree = Shell({ children: React.createElement("div", null, "content") });
    const clickable = findElements(shellTree, (el) => typeof el.props?.onClick === "function");
    clickable.forEach((el) => el.props.onClick?.());

    mocks.usePathname.mockReturnValue("/");
    renderToStaticMarkup(React.createElement(Shell, null, React.createElement("div", null, "root")));

    expect(mocks.trackNavigationClicked).toHaveBeenCalled();
    // Brand-mark + wordmark are the topbar shape — Operational Restraint:
    // no decorative badges, no marketing chrome.
    const markup = renderToStaticMarkup(React.createElement(React.Fragment, null, shellTree));
    expect(markup).toContain("usezombie");
    expect(markup).toContain("data-live");
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
    mocks.usePathname.mockReturnValue("/zombies");
    const tree = Shell({ children: React.createElement("div", null, "content") });
    const markup = renderToStaticMarkup(React.createElement(React.Fragment, null, tree));
    // Brand-mark always-alive contract.
    expect(markup).toContain("data-live");
    expect(markup).toContain("usezombie");
    // Sidebar nav rendered with all 5 operational routes + 2 in More.
    expect(markup).toContain("Dashboard");
    expect(markup).toContain("Zombies");
    expect(markup).toContain("Credentials");
    expect(markup).toContain("Approvals");
    expect(markup).toContain("Events");
    expect(markup).toContain("Settings");
  });

  it("Shell sidebar marks the active route via data-active attribute", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/zombies");
    const tree = Shell({ children: React.createElement("div") });
    const markup = renderToStaticMarkup(React.createElement(React.Fragment, null, tree));
    // The active link gets data-active="true" — the sidebar's surface-3 fill
    // is driven from this attribute (no coloured bar per spec).
    expect(markup).toMatch(/data-active="true"[^>]*>\s*<svg[^>]*data-icon="SkullIcon"/);
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
    expect(dialog.textContent).toContain("Zombies");
    cleanup();
  });

  it("emits navigation analytics from sidebar, bottom-nav, and header links", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    render(
      React.createElement(Shell, null, React.createElement("div", null, "content")),
    );
    // Sidebar 'Dashboard' is href "/" → the source uses the 'root' branch;
    // 'Zombies' exercises the path-to-slug replaceAll branch.
    await user.click(screen.getByText("Dashboard"));
    await user.click(screen.getByText("Zombies"));
    // Bottom group: 'Docs' is external (anchor branch), 'Settings' internal.
    await user.click(screen.getByText("Docs"));
    await user.click(screen.getByText("Settings"));
    // Header marketing/docs anchors.
    await user.click(screen.getByText("docs"));
    await user.click(screen.getByText("usezombie.com"));

    const sources = mocks.trackNavigationClicked.mock.calls.map(
      (c) => (c[0] as { source: string }).source,
    );
    expect(sources).toContain("app_sidebar_root");
    expect(sources).toContain("app_sidebar_zombies");
    expect(sources).toContain("app_sidebar_more_docs");
    expect(sources).toContain("app_sidebar_more_settings");
    expect(sources).toContain("app_header_docs");
    expect(sources).toContain("app_header_marketing");
    cleanup();
  });
});
