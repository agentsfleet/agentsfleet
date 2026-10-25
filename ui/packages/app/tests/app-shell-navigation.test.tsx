import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const mocks = vi.hoisted(() => ({ pathname: vi.fn(), track: vi.fn() }));
vi.mock("@/lib/analytics/posthog", () => ({ trackNavigationClicked: mocks.track, setAnalyticsContext: vi.fn(), captureProductEvent: vi.fn() }));
vi.mock("@/lib/auth/client", () => ({ useCurrentUser: () => ({ userId: "person" }), AuthSignOutButton: ({ children }: { children: React.ReactNode }) => children }));
vi.mock("next/navigation", () => ({ usePathname: mocks.pathname, useSearchParams: () => new URLSearchParams(), useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }) }));
import { ShellFrame } from "@/components/layout/ShellFrame";
import { OWN_ACCOUNT } from "@/tests/helpers/workspace-fixtures";
import { SidebarNavigation } from "@/components/layout/SidebarNavigation";
import { NavSection } from "@/components/layout/SidebarNavigationSections";
import { BOTTOM_NAV } from "@/components/layout/account-menu-data";
import { navSource } from "@/components/layout/sidebar-navigation-data";

const WORKSPACE = { id: "ws_1", name: "My workspace", created_at: "1970-01-01T00:00:00.001Z", ...OWN_ACCOUNT };

it("keeps explicit navigation analytics stable and derives a fallback for other external links", () => {
  expect(navSource(BOTTOM_NAV[0]!)).toBe("app_sidebar_docs");
  expect(navSource({ ...BOTTOM_NAV[0]!, label: "Reference", analyticsSource: undefined })).toBe("app_sidebar_reference");
});
const MENU = /Workspace and account menu/;
function mount(scopes: string[] = []) { render(<ShellFrame workspaces={[WORKSPACE]} operatorScopes={scopes}>Page content</ShellFrame>); }
beforeEach(() => { mocks.pathname.mockReturnValue("/w/ws_1/fleets"); });
afterEach(() => { cleanup(); vi.clearAllMocks(); });

async function openMenu() {
  await userEvent.setup().click(screen.getByRole("button", { name: MENU }));
  return screen.findByRole("menu");
}

describe("conversation-first app navigation", () => {
  it("opens account navigation without workspaces and preserves external link semantics", () => {
    const view = render(<SidebarNavigation pathname="/settings/account" workspaceId={null} operatorScopes={[]} collapsed onNavigate={() => {}} />);
    expect(screen.getByRole("button", {name: MENU})).toBeTruthy();
    view.unmount();
    render(<NavSection items={BOTTOM_NAV} activeHref="" workspaceId={null} collapsed={false} onNavigate={() => {}} />);
    const docs = screen.getByRole("link", {name: "Learn more"});
    expect(docs.getAttribute("target")).toBe("_blank");
    expect(docs.getAttribute("rel")).toBe("noopener noreferrer");
  });
  it("uses the compact centered New fleet icon button when collapsed", () => {
    render(<SidebarNavigation pathname="/w/ws_1/fleets" workspaceId={WORKSPACE.id} operatorScopes={[]} collapsed onNavigate={() => {}} />);
    const action = screen.getByRole("link", {name: "New fleet"});
    expect(action.classList.contains("justify-center")).toBe(true);
    expect(action.querySelector("svg")?.getAttribute("width")).toBe("18");
    expect(action.getAttribute("href")).toBe("/w/ws_1/fleets/new");
    expect(action.getAttribute("title")).toBe("New fleet");
  });
  it("keeps the expanded sidebar free of search and fleet recents", () => {
    mount();
    expect(screen.getByRole("link", {name: "Your fleets"})).toBeTruthy();
    expect(screen.queryByRole("textbox", {name: "Search all chats"})).toBeNull();
    expect(screen.queryByText("Recents")).toBeNull();
  });
  it("opens New fleet and records account and documentation destinations", async () => {
    mount(); const user = userEvent.setup();
    await user.click(screen.getByRole("link", {name: "New fleet"}));
    expect(mocks.track).toHaveBeenCalledWith({source: "app_sidebar_fleets_new", surface: "app_sidebar", target: "/w/ws_1/fleets/new"});
    await openMenu();
    for (const [name, href] of [["Usage", "/settings/billing"], ["Connect apps", "/w/ws_1/integrations"], ["Get API keys", "/settings/api-keys"], ["Manage secrets", "/w/ws_1/secrets"]]) {
      expect(screen.getByRole("menuitem", {name}).getAttribute("href")).toBe(href);
    }
    const docs = screen.getByRole("menuitem", {name: "Learn more"});
    expect(docs.getAttribute("rel")).toBe("noopener noreferrer");
    document.addEventListener("click", (event) => event.preventDefault(), {once: true});
    await user.click(docs);
    expect(screen.queryByRole("menu")).toBeNull();
    expect(mocks.track).toHaveBeenCalledWith({source: "app_sidebar_docs", surface: "app_sidebar", target: "https://docs.agentsfleet.net"});
    await openMenu(); await user.click(screen.getByRole("menuitem", {name: "Settings"}));
    await user.click(screen.getByRole("menuitem", {name: "Profile"}));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(mocks.track).toHaveBeenCalledWith({source: "app_sidebar_settings_account", surface: "app_sidebar", target: "/settings/account"});
  });
  it("keeps the two primary destinations and moves settings off the canvas", () => {
    mount();
    expect(screen.getByRole("link", { name: "New fleet" }).getAttribute("href")).toBe("/w/ws_1/fleets/new");
    expect(screen.getByRole("link", { name: "Your fleets" }).getAttribute("aria-current")).toBe("page");
    expect(screen.queryByText("Activity")).toBeNull();
    expect(screen.queryByText("Workspace settings")).toBeNull();
    expect(screen.getByRole("button", { name: MENU }).closest("aside")).not.toBeNull();
  });
  it("places Activity under Workspace and hides an empty Platform menu", async () => {
    mount(); await openMenu();
    expect(screen.queryByRole("menuitem", {name: "Platform"})).toBeNull();
    await userEvent.setup().click(screen.getByRole("menuitem", {name: "Settings"}));
    expect(screen.getByText("Workspace")).toBeTruthy();
    expect(screen.getByRole("menuitem", {name: "Activity"}).getAttribute("href")).toBe("/w/ws_1/events");
    expect(screen.queryByRole("menuitem", {name: "Runners"})).toBeNull();
  });
  it("adds only the operator destinations permitted by the session", async () => {
    mount(["runner:read", "platform-library:write"]); await openMenu();
    await userEvent.setup().click(screen.getByRole("menuitem", { name: "Platform" }));
    expect(screen.getByRole("menuitem", { name: "Runners" }).getAttribute("href")).toBe("/admin/runners");
    expect(screen.queryByText("Model library")).toBeNull();
  });
  it("opens the selected workspace's approvals inbox without operator privileges", async () => {
    mocks.pathname.mockReturnValue("/w/ws_1/approvals/gate-1");
    mount(); await openMenu();
    await userEvent.setup().click(screen.getByRole("menuitem", {name: "Settings"}));
    const approvals = screen.getByRole("menuitem", {name: "Approvals"});
    expect(approvals.getAttribute("href")).toBe("/w/ws_1/approvals");
    expect(approvals.getAttribute("aria-current")).toBe("page");
    expect(screen.getAllByRole("menuitem").filter((item) => item.getAttribute("aria-current") === "page")).toHaveLength(1);
    await userEvent.setup().click(approvals);
    expect(screen.queryByRole("menu")).toBeNull();
    expect(mocks.track).toHaveBeenCalledWith({source: "app_sidebar_approvals", surface: "app_sidebar", target: "/w/ws_1/approvals"});
  });
  it("keeps settings keyboard accessible and restores the workspace view", async () => {
    mount(); await openMenu();
    const user = userEvent.setup();
    await user.click(screen.getByRole("menuitem", { name: "Settings" }));
    const back = screen.getByRole("menuitem", { name: "Back" });
    expect(document.activeElement).toBe(back);
    expect(screen.getByRole("menuitem", { name: "Models" }).getAttribute("href")).toBe("/w/ws_1/settings/models");
    expect(screen.getByText("Workspace")).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Fleet library" }).getAttribute("href")).toBe("/w/ws_1/library");
    expect(screen.getByRole("menuitem", { name: "Team" }).getAttribute("href")).toBe("/settings/members");
    for (const name of ["API keys", "Integrations", "Secrets", "Billing"]) {
      expect(screen.queryByRole("menuitem", { name })).toBeNull();
    }
    await user.click(back);
    expect(screen.getByRole("menuitem", { name: /My workspace/ })).toBeTruthy();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
  });
  it("keeps collapsed controls named and opens full navigation on mobile", async () => {
    mount(); const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    expect(screen.queryByText("Your fleets")).toBeNull();
    expect(screen.getByRole("link", { name: "Your fleets" })).toBeTruthy();
    expect(screen.getByRole("button", { name: MENU })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Open navigation" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Your fleets")).toBeTruthy();
    await user.click(within(dialog).getByRole("link", { name: "Your fleets" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

it("marks the active Platform destination and keeps menu icons decorative", async () => {
  mocks.pathname.mockReturnValue("/admin/runners/runner-1");
  mount(["runner:read", "model:read"]);
  await openMenu();
  await userEvent.click(screen.getByRole("menuitem", {name: "Platform"}));
  expect(screen.getByRole("menuitem", {name: "Runners"}).getAttribute("aria-current")).toBe("page");
  expect(screen.getByRole("menuitem", {name: "Model library"}).hasAttribute("aria-current")).toBe(false);
  for (const item of screen.getAllByRole("menuitem")) {
    expect(item.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
  }
});
