import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

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
    expect(resolveActiveHref(["/w/1/fleets"], "/unmatched")).toBe("");
  });
});

describe("app shell frame", () => {
  it("keeps home and fleet entrances in the persistent rail", async () => {
    const { ShellFrame: Shell } = await import("../components/layout/ShellFrame");
    mocks.usePathname.mockReturnValue("/w/ws_1/fleets");
    const { container } = render(React.createElement(Shell, null, React.createElement("div", null, "Route content")));
    const aside = container.querySelector("aside");
    expect(aside?.contains(screen.getByRole("link", {name: "agentsfleet home"}))).toBe(true);
    expect(screen.getAllByRole("link", {name: "New fleet"}).some(link => aside?.contains(link))).toBe(true);
    expect(screen.getByRole("main").textContent).toContain("Route content");
  });
});
