import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";

const mocks = vi.hoisted(() => ({
  trackNavigationClicked: vi.fn(), useUser: vi.fn(), usePathname: vi.fn(),
}));

vi.mock("@/lib/analytics/posthog", () => ({
  trackAppEvent: vi.fn(), trackNavigationClicked: mocks.trackNavigationClicked,
  setAnalyticsContext: vi.fn(), captureProductEvent: vi.fn(),
}));
vi.mock("@clerk/nextjs", () => ({
  SignIn: () => null, SignUp: () => null,
  SignOutButton: ({ children }: { children: React.ReactNode }) => children, UserProfile: () => null,
  UserButton: () => React.createElement("div"), useUser: mocks.useUser,
  ClerkProvider: ({ children }: { children: React.ReactNode }) => React.createElement(React.Fragment, null, children),
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
  mocks.trackNavigationClicked.mockReset();
  mocks.usePathname.mockReturnValue("/workspaces");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("resolveActiveHref — single active winner", () => {
  it("returns the longest href that prefixes the path, so a nested route never lights a sibling", async () => {
    const { resolveActiveHref } = await import("../components/layout/SidebarNavigation");
    // The invariant that guards against a future nav pair where one path
    // prefixes another: the deeper route lights only its own href.
    expect(
      resolveActiveHref(["/w/1/settings", "/w/1/settings/models"], "/w/1/settings/models"),
    ).toBe("/w/1/settings/models");
    // A resource-detail route collapses onto its section href.
    expect(resolveActiveHref(["/w/1/fleets"], "/w/1/fleets/abc")).toBe("/w/1/fleets");
    // Exact match on a plain sibling.
    expect(resolveActiveHref(["/w/1/fleets", "/w/1/events"], "/w/1/events")).toBe("/w/1/events");
    // Entry-redirect stubs never win, and an unmatched path lights nothing.
    expect(resolveActiveHref(["/", ""], "/")).toBe("");
    expect(resolveActiveHref(["/w/1/fleets"], "/admin/runners")).toBe("");
  });
});

describe("app shell frame", () => {
  it("test_the_header_cluster_is_the_sidebar_column", async () => {
    const { SIDEBAR_COLUMN, shellSidebarState } = await import(
      "../components/layout/shell-sidebar-state"
    );

    // The toggle lines up with the column it collapses only while the header
    // cluster and the `<aside>` are the same width. One constant, two literal
    // spellings (Tailwind scans source text, so `md:${...}` would never reach
    // the stylesheet) — this is what stops the pair drifting apart.
    expect(SIDEBAR_COLUMN.header.expanded).toBe(`md:${SIDEBAR_COLUMN.aside.expanded}`);
    expect(SIDEBAR_COLUMN.header.collapsed).toBe(`md:${SIDEBAR_COLUMN.aside.collapsed}`);

    const { ShellFrame: Shell } = await import("../components/layout/ShellFrame");
    mocks.usePathname.mockReturnValue("/w/ws_1/fleets");
    const { container } = render(React.createElement(Shell, null, React.createElement("div")));

    // The header pads nothing itself: the leading cluster has to start where
    // the sidebar column starts, so each cluster pads its own side instead.
    const header = container.querySelector("header") as HTMLElement;
    expect(header.className).toContain("px-0");
    expect(header.className).toContain("md:px-0");

    const cluster = container.querySelector(
      '[data-testid="shell-leading-cluster"]',
    ) as HTMLElement;
    const brand = container.querySelector('[aria-label="agentsfleet home"]') as HTMLElement;

    // Expanded: the cluster IS the column, brand on the nav items' leading
    // edge and toggle on their trailing edge.
    expect(cluster.className).toContain(SIDEBAR_COLUMN.header.expanded);
    expect(cluster.className).toContain("md:justify-between");
    expect(cluster.className).toContain("md:px-3");
    expect(brand.className).not.toContain("md:hidden");

    // Collapsed: a 64px rail holds one control. The wordmark stands down from
    // `md` up only — the mobile header has no rail and keeps its brand.
    act(() => shellSidebarState.setCollapsed(true));
    expect(cluster.className).toContain(SIDEBAR_COLUMN.header.collapsed);
    expect(cluster.className).toContain("md:justify-center");
    expect(brand.className).toContain("md:hidden");

    act(() => shellSidebarState.reset());
  });
});
