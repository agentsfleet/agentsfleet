import React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const notFound = vi.fn(() => {
  throw new Error("notFound");
});
const usePathname = vi.fn(() => "/");
const auth = vi.fn();
const useUser = vi.fn(() => ({ isLoaded: false, user: null }));

vi.mock("next/navigation", () => ({
  redirect,
  notFound,
  usePathname,
  useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
}));

vi.mock("@/lib/api/workspaces", () => ({
  listTenantWorkspaces: vi.fn().mockResolvedValue({ items: [], total: 0 }),
}));

vi.mock("@clerk/nextjs/server", () => ({
  auth,
}));

vi.mock("@clerk/nextjs", () => ({
  ClerkProvider: ({ children }: { children: React.ReactNode }) => React.createElement(React.Fragment, null, children),
  UserButton: () => React.createElement("div", { "data-user-button": "1" }),
  SignIn: ({ appearance }: { appearance: unknown }) => React.createElement("div", { "data-sign-in": JSON.stringify(appearance) }),
  SignUp: ({ appearance }: { appearance: unknown }) => React.createElement("div", { "data-sign-up": JSON.stringify(appearance) }),
  useUser,
}));

vi.mock("@/components/analytics/AnalyticsPageEvent", () => ({
  default: ({ event }: { event: string }) =>
    React.createElement("div", { "data-analytics-event": event }),
}));

vi.mock("@/components/analytics/TrackedAnchor", () => ({
  default: ({ href, children, event }: { href: string; children: React.ReactNode; event: string }) =>
    React.createElement("a", { href, "data-track-event": event }, children),
}));

vi.mock("lucide-react", () => ({
  PlusIcon: () => React.createElement("svg", { "data-icon": "PlusIcon" }),
  LayoutDashboardIcon: () => React.createElement("svg", { "data-icon": "LayoutDashboardIcon" }),
  SkullIcon: () => React.createElement("svg", { "data-icon": "SkullIcon" }),
  ActivityIcon: () => React.createElement("svg", { "data-icon": "ActivityIcon" }),
  SettingsIcon: () => React.createElement("svg", { "data-icon": "SettingsIcon" }),
  BookOpenIcon: () => React.createElement("svg", { "data-icon": "BookOpenIcon" }),
  ZapIcon: () => React.createElement("svg", { "data-icon": "ZapIcon" }),
  ShieldIcon: () => React.createElement("svg", { "data-icon": "ShieldIcon" }),
  KeyRoundIcon: () => React.createElement("svg", { "data-icon": "KeyRoundIcon" }),
  CheckCircle2Icon: () => React.createElement("svg", { "data-icon": "CheckCircle2Icon" }),
}));

vi.mock("@/lib/workspace", () => ({
  resolveActiveWorkspace: vi.fn().mockResolvedValue(null),
  listTenantWorkspacesCached: vi.fn().mockResolvedValue({ items: [], total: 0 }),
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

    const rootMarkup = renderToStaticMarkup(React.createElement(RootLayout, null, React.createElement("div", null, "root child")));
    const dashboardMarkup = renderToStaticMarkup(
      await DashboardLayout({ children: React.createElement("div", null, "dash child") }),
    );
    const authMarkup = renderToStaticMarkup(React.createElement(AuthLayout, null, React.createElement("div", null, "auth child")));

    expect(rootMarkup).toContain("root child");
    expect(dashboardMarkup).toContain("dash child");
    expect(authMarkup).toContain("usezombie");
    expect(authMarkup).toContain("Mission Control");
  });

  it("dashboard entry page renders page header when authenticated", async () => {
    const { default: DashboardPage } = await import("../app/(dashboard)/page");
    const markup = renderToStaticMarkup(await DashboardPage());
    expect(markup).toContain("Dashboard");
  });

  it("renders sign-in and sign-up pages with shared auth appearance", async () => {
    const { default: SignInPage } = await import("../app/(auth)/sign-in/[[...sign-in]]/page");
    const { default: SignUpPage } = await import("../app/(auth)/sign-up/[[...sign-up]]/page");

    const signInMarkup = renderToStaticMarkup(React.createElement(SignInPage));
    const signUpMarkup = renderToStaticMarkup(React.createElement(SignUpPage));

    expect(signInMarkup).toContain("data-sign-in=");
    expect(signUpMarkup).toContain("data-sign-up=");
    expect(signInMarkup).toContain("colorPrimary");
    expect(signUpMarkup).toContain("colorPrimary");
  });
});
