import React from "react";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
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
  UserButton: () => React.createElement("div"), useUser: mocks.useUser,
  ClerkProvider: ({ children }: { children: React.ReactNode }) => React.createElement(React.Fragment, null, children),
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
    KeyRoundIcon: icon("KeyRoundIcon"), LibraryIcon: icon("LibraryIcon"), PlugIcon: icon("PlugIcon"), CheckCircle2Icon: icon("CheckCircle2Icon"), ServerIcon: icon("ServerIcon"),
    BrainCircuitIcon: icon("BrainCircuitIcon"), BoxesIcon: icon("BoxesIcon"), CreditCardIcon: icon("CreditCardIcon"), MenuIcon: icon("MenuIcon"),
    PanelLeftCloseIcon: icon("PanelLeftCloseIcon"), PanelLeftOpenIcon: icon("PanelLeftOpenIcon"), SunIcon: icon("SunIcon"), MoonIcon: icon("MoonIcon"), ChevronDownIcon: icon("ChevronDownIcon"), ChevronRightIcon: icon("ChevronRightIcon"), PlusIcon: icon("PlusIcon"), FolderIcon: icon("FolderIcon"),
  };
});
vi.mock("@/components/layout/ThemeToggle", () => ({ default: () => React.createElement("button") }));
vi.mock("@/components/layout/ClientOnlyAuthUserButton", () => ({ default: () => React.createElement("div") }));

beforeEach(() => {
  mocks.useUser.mockReset();
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
  it("test_dashboard_shell_hydrates_only_interactive_islands", () => {
    const appRoot = resolve(import.meta.dirname, "..");
    const layout = readFileSync(
      resolve(appRoot, "app/(dashboard)/layout.tsx"),
      "utf8",
    );
    const frame = readFileSync(
      resolve(appRoot, "components/layout/ShellFrame.tsx"),
      "utf8",
    );
    const controls = readFileSync(
      resolve(appRoot, "components/layout/ShellControls.tsx"),
      "utf8",
    );
    const sidebar = readFileSync(
      resolve(appRoot, "components/layout/SidebarNavigation.tsx"),
      "utf8",
    );

    expect(frame.startsWith('"use client"')).toBe(false);
    expect(frame).toContain("{children}");
    expect(layout).toContain("<ShellFrame");
    // Reverses the original "no provider in the layout" rule from
    // `perf(app): make authenticated routes fluid`. That rule was a bundle
    // argument and it was correct: hoisting the provider moves the Radix
    // tooltip runtime into the shared chunk every dashboard route loads, worth
    // ~18.6 KiB gzipped (252.04 → 271.66), and `.size-limit.mjs` was raised to
    // match. It is paid on purpose. Eight islands each carried their own
    // provider, and the one that forgot took its whole page down — Radix's
    // tooltip Root reads provider context unconditionally and THROWS rather
    // than degrading. Asserted positively so removing the provider fails here
    // rather than silently reintroducing that crash.
    expect(layout).toContain("<TooltipProvider");
    expect(controls.startsWith('"use client"')).toBe(true);
    expect(controls).toContain('import("./MobileNavigationDialog")');
    expect(sidebar).toContain("GettingStartedWidget");
    expect(sidebar).toContain('from "./GettingStartedWidget"');
    expect(sidebar).not.toContain("GettingStartedWidgetDynamic");
    expect(
      existsSync(resolve(appRoot, "components/layout/Shell.tsx")),
    ).toBe(false);
  });

  it("test_shell_respects_session_keeper_verdict", () => {
    const appRoot = resolve(import.meta.dirname, "..");
    const rootLayout = readFileSync(
      resolve(appRoot, "app/layout.tsx"),
      "utf8",
    );
    const dashboardLayout = readFileSync(
      resolve(appRoot, "app/(dashboard)/layout.tsx"),
      "utf8",
    );
    const frame = readFileSync(
      resolve(appRoot, "components/layout/ShellFrame.tsx"),
      "utf8",
    );

    expect(rootLayout).toContain(
      'import { AuthProvider, AuthSessionKeeper } from "@/lib/auth/client"',
    );
    expect(rootLayout).toContain("<AuthSessionKeeper />");
    expect(dashboardLayout).not.toContain("AuthSessionKeeper");
    expect(frame).not.toContain("AuthSessionKeeper");
  });

  it("is a fixed frame whose content region owns the scroll", async () => {
    const { ShellFrame: Shell } = await import("../components/layout/ShellFrame");
    mocks.usePathname.mockReturnValue("/w/ws_1/fleets");
    const { container } = render(React.createElement(Shell, null, React.createElement("div")));

    // A growing document cannot host a surface that pins its own composer:
    // the page scrolls and the composer leaves the viewport with it.
    const frame = container.querySelector('[data-surface="dashboard"]') as HTMLElement;
    expect(frame.className).toMatch(/h-dvh/);
    expect(frame.className).toMatch(/fixed/);
    expect(frame.className).toMatch(/inset-0/);
    expect(frame.className).not.toMatch(/min-h-screen/);

    const header = container.querySelector("header") as HTMLElement;
    expect(header.className).not.toMatch(/border-b/);
    expect(header.className).toContain("after:h-px");

    const main = container.querySelector("main") as HTMLElement;
    expect(main.className).toMatch(/overflow-y-auto/);
    expect(main.className).toMatch(/min-h-0/);
  });

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

  it("lets an ordinary page grow while letting one page claim the region", async () => {
    const { ShellFrame: Shell } = await import("../components/layout/ShellFrame");
    mocks.usePathname.mockReturnValue("/w/ws_1/fleets");
    const { container } = render(React.createElement(Shell, null, React.createElement("div")));

    // `min-h-full` + column flow: tall content still scrolls the region, and a
    // child asking for `flex-1` fills it exactly instead.
    const canvas = container.querySelector("main > div") as HTMLElement;
    expect(canvas.className).toMatch(/min-h-full/);
    expect(canvas.className).toMatch(/flex-col/);
  });
});
