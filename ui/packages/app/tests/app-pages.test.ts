import React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const notFound = vi.fn(() => {
  throw new Error("notFound");
});
const usePathname = vi.fn(() => "/w/ws_1");
const auth = vi.fn();
const useUser = vi.fn(() => ({ isLoaded: false, user: null }));
const clerkProvider = vi.fn(({ children }: { children: React.ReactNode }) => React.createElement(React.Fragment, null, children));

vi.mock("next/navigation", () => ({
  redirect,
  notFound,
  useSearchParams: () => new URLSearchParams(), usePathname,
  useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
}));

vi.mock("@/lib/api/workspaces", () => ({
  listTenantWorkspaces: vi.fn().mockResolvedValue({ items: [], total: 0 }),
}));

vi.mock("@clerk/nextjs/server", () => ({
  auth,
}));

vi.mock("@clerk/nextjs", () => ({
  ClerkProvider: clerkProvider,
  UserProfile: ({ path, routing, appearance }: { path: string; routing: string; appearance: unknown }) => React.createElement("div", { "data-user-profile": path, "data-routing": routing, "data-appearance": JSON.stringify(appearance) }),
  SignOutButton: ({ children }: { children: React.ReactNode }) => children,
  UserButton: () => React.createElement("div", { "data-user-button": "1" }),
  SignIn: ({ fallback }: { fallback: React.ReactNode }) => React.createElement("div", { "data-sign-in": "" }, fallback),
  SignUp: ({ fallback }: { fallback: React.ReactNode }) => React.createElement("div", { "data-sign-up": "" }, fallback),
  useClerk: () => ({status: "loading"}),
  useUser,
}));

// M118: `lib/workspace` is slimmed to a single survivor — the active workspace
// is now an explicit `/w/<id>` URL segment, not a resolved cookie/claim. The
// dashboard layout still fetches the full owned list for the switcher; seed one
// so the nav resolves its workspace-scoped hrefs under `/w/ws_1/…`.
vi.mock("@/lib/workspace", () => ({
  listTenantWorkspacesCached: vi
    .fn()
    .mockResolvedValue({
      items: [{ id: "ws_1", name: "Alpha", created_at: "1970-01-01T00:00:00.001Z", account: { tenant_id: "tenant_own", owner_name: "You" }, role: "owner" }],
      total: 1,
    }),
}));


beforeEach(() => {
  vi.clearAllMocks();
  auth.mockResolvedValue({
    getToken: vi.fn().mockResolvedValue("token_123"),
  });
});

describe("app layouts and pages", () => {
  it("wraps root and dashboard layouts and renders auth layout branding", async () => {
    const { default: RootLayout } = await import("../app/layout");
    const { default: DashboardLayout } = await import("../app/(dashboard)/layout");
    const { default: AuthLayout } = await import("../app/(auth)/layout");

    const rootMarkup = renderToStaticMarkup(
      RootLayout({ children: React.createElement("div", null, "root child") }),
    );
    const dashboardMarkup = renderToStaticMarkup(
      await DashboardLayout({ children: React.createElement("div", null, "dash child") }),
    );
    const authMarkup = renderToStaticMarkup(React.createElement(AuthLayout, null, React.createElement("div", null, "auth child")));

    expect(rootMarkup).toContain("root child");
    expect(clerkProvider.mock.calls[0]?.[0]).toEqual(expect.objectContaining({
      appearance: AUTH_APPEARANCE,
      signInUrl: "/sign-in", signUpUrl: "/sign-up",
      signInFallbackRedirectUrl: "/", signUpFallbackRedirectUrl: "/",
    }));
    expect(dashboardMarkup).toContain("dash child");
    expect(authMarkup).toContain("agentsfleet");
    expect(authMarkup).toContain('data-brand-mark="signal"');
    expect(authMarkup).not.toContain("data-live");
    // Configuration nav carries the consolidated Models destination
    // (the standalone Credentials vault was folded into it).
    expect(dashboardMarkup).toContain("Connected apps");
    // M118: the Models destination is workspace-scoped — it carries the `/w/<id>`
    // segment (the switcher-seeded first workspace) rather than a bare root path.
    expect(dashboardMarkup).toContain('href="/w/ws_1/fleets/new"');
    expect(dashboardMarkup).not.toContain('href="/credentials"');
  });

  it("renders a route-specific loading state with each native auth widget", async () => {
    const { default: SignInPage } = await import("../app/(auth)/sign-in/[[...sign-in]]/page");
    const { default: SignUpPage } = await import("../app/(auth)/sign-up/[[...sign-up]]/page");

    const signInMarkup = renderToStaticMarkup(React.createElement(SignInPage));
    const signUpMarkup = renderToStaticMarkup(React.createElement(SignUpPage));

    expect(signInMarkup).toContain("data-sign-in=");
    expect(signUpMarkup).toContain("data-sign-up=");
    expect(signInMarkup).toContain("Loading sign-in…");
    expect(signUpMarkup).toContain("Loading sign-up…");
  });
});
