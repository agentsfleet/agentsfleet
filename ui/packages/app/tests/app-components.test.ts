import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

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
  it("tracks analytics wrapper components and shell navigation", async () => {
    const { default: AnalyticsPageEvent } = await import("../components/analytics/AnalyticsPageEvent");
    const { default: TrackedAnchor } = await import("../components/analytics/TrackedAnchor");
    const { default: Shell } = await import("../components/layout/Shell");

    AnalyticsPageEvent({ event: "workspace_list_viewed", properties: { surface: "workspace_list" } });
    const trackedAnchor = TrackedAnchor({
      event: "workspace_action_clicked",
      properties: { target: "pause" },
      onClick: vi.fn(),
      href: "/workspaces/ws_1/pause",
      children: "Pause",
    });
    trackedAnchor.props.onClick?.({ type: "click" } as unknown as React.MouseEvent<HTMLAnchorElement>);

    const shellTree = Shell({ children: React.createElement("div", null, "content") });
    const clickable = findElements(shellTree, (el) => typeof el.props?.onClick === "function");
    clickable.forEach((el) => el.props.onClick?.());

    mocks.usePathname.mockReturnValue("/");
    renderToStaticMarkup(React.createElement(Shell, null, React.createElement("div", null, "root")));

    expect(mocks.trackAppEvent).toHaveBeenCalledWith("workspace_list_viewed", { surface: "workspace_list" });
    expect(mocks.trackAppEvent).toHaveBeenCalledWith("workspace_action_clicked", { target: "pause" });
    expect(mocks.trackNavigationClicked).toHaveBeenCalled();
    expect(renderToStaticMarkup(React.createElement(React.Fragment, null, shellTree))).toContain("Mission Control");
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

    expect(AUTH_APPEARANCE.variables.colorPrimary).toBe("var(--z-orange)");
    expect(AUTH_APPEARANCE.elements.formButtonPrimary.color).toBe("var(--z-text-inverse)");
    expect(AUTH_APPEARANCE.elements.footer.background).toContain("linear-gradient");
  });
});
