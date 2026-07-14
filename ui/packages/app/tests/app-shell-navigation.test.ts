import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

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
  mocks.trackNavigationClicked.mockReset();
  mocks.usePathname.mockReturnValue("/workspaces");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("app shell navigation", () => {
  it("appends the Runners item only when the session holds runner:read", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const markup = renderToStaticMarkup(
      React.createElement(
        Shell,
        { operatorScopes: ["runner:read"] } as React.ComponentProps<typeof Shell>,
        React.createElement("div"),
      ),
    );
    expect(markup).toContain("Configuration");
    expect(markup).toContain("Runners");
    expect(markup).toContain('href="/admin/runners"');
    expect(markup).toContain('data-icon="ServerIcon"');
    expect((markup.match(/>Configuration</g) ?? []).length).toBe(1);
    expect(markup).not.toMatch(/>\s*Platform\s*</);
  });

  it("hides the platform surface for a session without operator scopes", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const markup = renderToStaticMarkup(React.createElement(Shell, null, React.createElement("div")));
    expect(markup).not.toContain('href="/admin/runners"');
    expect(markup).not.toContain('data-icon="ServerIcon"');
    expect(markup).not.toContain('href="/admin/fleet-libraries"');
  });

  it("appends Fleet library only for platform-library:write", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const withScope = renderToStaticMarkup(
      React.createElement(
        Shell,
        { operatorScopes: ["platform-library:write"] } as React.ComponentProps<typeof Shell>,
        React.createElement("div"),
      ),
    );
    expect(withScope).toContain("Fleet library");
    expect(withScope).toContain('href="/admin/fleet-libraries"');

    const otherScope = renderToStaticMarkup(
      React.createElement(
        Shell,
        { operatorScopes: ["model:admin"] } as React.ComponentProps<typeof Shell>,
        React.createElement("div"),
      ),
    );
    expect(otherScope).not.toContain('href="/admin/fleet-libraries"');
  });

  it("renders the mobile navigation button", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));
    const hamburger = screen.getByRole("button", { name: /open navigation/i });
    expect(hamburger).toBeTruthy();
    expect(hamburger.className).toContain("md:hidden");
  });

  it("opens the sidebar navigation from the mobile button", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));
    await user.click(screen.getByRole("button", { name: /open navigation/i }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toBeTruthy();
    expect(dialog.textContent).toContain("Dashboard");
    expect(dialog.textContent).toContain("Fleets");
  });

  it("closes the mobile dialog after navigation", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/fleets");
    const user = userEvent.setup();
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));
    await user.click(screen.getByRole("button", { name: /open navigation/i }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("link", { name: /dashboard/i }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("emits navigation analytics from sidebar and bottom-nav links", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));
    await user.click(screen.getByText("Dashboard"));
    await user.click(screen.getByText("Fleets"));
    await user.click(screen.getByText("Docs"));
    await user.click(screen.getByText("API Keys"));
    await user.click(screen.getByText("Models"));
    await user.click(screen.getByText("Billing"));

    const sources = mocks.trackNavigationClicked.mock.calls.map(
      (call) => (call[0] as { source: string }).source,
    );
    expect(sources).toContain("app_sidebar_root");
    expect(sources).toContain("app_sidebar_fleets");
    expect(sources).toContain("app_sidebar_docs");
    expect(sources).toContain("app_sidebar_settings_api-keys");
    expect(sources).toContain("app_sidebar_settings_models");
    expect(sources).toContain("app_sidebar_settings_billing");
  });

  it("collapses the sidebar and hides navigation labels", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));
    const toggle = screen.getByRole("button", { name: /collapse sidebar/i });
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText("Dashboard")).toBeTruthy();
    await user.click(toggle);
    const expand = screen.getByRole("button", { name: /expand sidebar/i });
    expect(expand).toBeTruthy();
    expect(screen.queryByRole("button", { name: /collapse sidebar/i })).toBeNull();
    expect(expand.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByText("Dashboard")).toBeNull();
  });

  it("expands the sidebar again when the toggle is clicked twice", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));
    await user.click(screen.getByRole("button", { name: /collapse sidebar/i }));
    expect(screen.queryByText("Dashboard")).toBeNull();
    await user.click(screen.getByRole("button", { name: /expand sidebar/i }));
    expect(screen.getByText("Dashboard")).toBeTruthy();
    expect(screen.getByRole("button", { name: /collapse sidebar/i })).toBeTruthy();
  });

  it("keeps navigation links accessible by name when collapsed", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/w/ws_1");
    const user = userEvent.setup();
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));
    await user.click(screen.getByRole("button", { name: /collapse sidebar/i }));
    const fleetsLink = screen.getByRole("link", { name: "Fleets" });
    expect(fleetsLink).toBeTruthy();
    expect(fleetsLink.getAttribute("href")).toBe("/w/ws_1/fleets");
  });

  it("hides section labels when collapsed", async () => {
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
  });

  it("renders the active link with pulse styling instead of generic accent", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/w/ws_1/fleets");
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));
    const activeLink = screen.getByRole("link", { name: "Fleets" });
    expect(activeLink.getAttribute("data-active")).toBe("true");
    expect(activeLink.className).toContain("data-[active=true]:bg-pulse/10");
    expect(activeLink.className).toContain("data-[active=true]:text-pulse");
    expect(activeLink.className).not.toContain("data-[active=true]:bg-accent");
  });

  it("renders a left accent bar on the active navigation item", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/w/ws_1/fleets");
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));
    const activeLink = screen.getByRole("link", { name: "Fleets" });
    expect(activeLink.className).toContain("border-l-2");
    expect(activeLink.className).toContain("border-transparent");
    expect(activeLink.className).toContain("data-[active=true]:border-pulse");
    const inactiveLink = screen.getByRole("link", { name: "Events" });
    expect(inactiveLink.getAttribute("data-active")).toBeNull();
    expect(inactiveLink.className).toContain("border-l-2");
  });

  it("keeps the mobile navigation expanded when the desktop sidebar is collapsed", async () => {
    const { default: Shell } = await import("../components/layout/Shell");
    mocks.usePathname.mockReturnValue("/");
    const user = userEvent.setup();
    render(React.createElement(Shell, null, React.createElement("div", null, "content")));
    await user.click(screen.getByRole("button", { name: /collapse sidebar/i }));
    await user.click(screen.getByRole("button", { name: /open navigation/i }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Dashboard")).toBeTruthy();
    expect(within(dialog).getByText("Fleets")).toBeTruthy();
  });
});
