import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const mocks = vi.hoisted(() => ({ pathname: vi.fn(), track: vi.fn(),
  identity: {userId: "person", displayName: undefined as string | undefined, emailAddress: undefined as string | undefined} }));
vi.mock("@/lib/analytics/posthog", () => ({ trackNavigationClicked: mocks.track, setAnalyticsContext: vi.fn(), captureProductEvent: vi.fn() }));
vi.mock("@/lib/auth/client", () => ({ useCurrentUser: () => mocks.identity, AuthSignOutButton: ({ children }: { children: React.ReactNode }) => children }));
vi.mock("next/navigation", () => ({ usePathname: mocks.pathname, useSearchParams: () => new URLSearchParams(), useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }) }));
import { ShellFrame } from "@/components/layout/ShellFrame";
import { OWN_ACCOUNT } from "@/tests/helpers/workspace-fixtures";
import { SidebarNavigation } from "@/components/layout/SidebarNavigation";
import { NavSection } from "@/components/layout/SidebarNavigationSections";
import { BookOpenIcon } from "@phosphor-icons/react/ssr";
import type { NavEntry } from "@/components/layout/sidebar-navigation-data";
import { navSource } from "@/components/layout/sidebar-navigation-data";
import { WorkspaceSwitcherTrigger } from "@/components/layout/WorkspaceSwitcherTrigger";

const REFERENCE_NAV: NavEntry[] = [{label: "Learn more", path: "https://docs.agentsfleet.net", icon: BookOpenIcon, external: true, analyticsSource: "app_sidebar_docs"}];
const WORKSPACE = { id: "ws_1", name: "My workspace", created_at: "1970-01-01T00:00:00.001Z", ...OWN_ACCOUNT };

it("keeps explicit navigation analytics stable and derives a fallback for other external links", () => {
  expect(navSource(REFERENCE_NAV[0]!)).toBe("app_sidebar_docs");
  expect(navSource({ ...REFERENCE_NAV[0]!, label: "Reference", analyticsSource: undefined })).toBe("app_sidebar_reference");
});
const MENU = /Workspace and account menu/;
function mount() { render(<ShellFrame workspaces={[WORKSPACE]}>Page content</ShellFrame>); }
beforeEach(() => { mocks.pathname.mockReturnValue("/w/ws_1/fleets"); Object.assign(mocks.identity, {displayName: undefined, emailAddress: undefined}); });
afterEach(() => { cleanup(); vi.clearAllMocks(); vi.unstubAllGlobals(); });

async function openMenu() {
  await userEvent.setup().click(screen.getByRole("button", { name: MENU }));
  return screen.findByRole("menu");
}

describe("conversation-first app navigation", () => {
  it("opens account navigation without workspaces and preserves external link semantics", () => {
    const view = render(<SidebarNavigation pathname="/settings/security" workspaceId={null} collapsed onNavigate={() => {}} />);
    expect(screen.getByRole("button", {name: MENU})).toBeTruthy();
    view.unmount();
    render(<NavSection items={REFERENCE_NAV} activeHref="" workspaceId={null} collapsed={false} onNavigate={() => {}} />);
    const docs = screen.getByRole("link", {name: "Learn more"});
    expect(docs.getAttribute("target")).toBe("_blank");
    expect(docs.getAttribute("rel")).toBe("noopener noreferrer");
  });
  it("names the compact New fleet icon and exposes its tooltip on keyboard focus", async () => {
    render(<SidebarNavigation pathname="/w/ws_1/fleets" workspaceId={WORKSPACE.id} collapsed onNavigate={() => {}} />);
    const action = screen.getAllByRole("link", {name: "New fleet"})[0]!;
    expect(action.classList.contains("justify-center")).toBe(true);
    expect(action.querySelector("svg")?.getAttribute("width")).toBe("20");
    expect(action.getAttribute("href")).toBe("/w/ws_1/fleets/new");
    const user = userEvent.setup();
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("link", {name: "agentsfleet home"}));
    await user.tab();
    expect(document.activeElement).toBe(action);
    expect(await screen.findByRole("tooltip", {name: "New fleet"})).toBeTruthy();
  });
  it("keeps the expanded sidebar free of search and fleet recents", () => {
    mount();
    expect(screen.getByRole("link", {name: "Your fleets"})).toBeTruthy();
    expect(screen.queryByRole("textbox", {name: "Search all chats"})).toBeNull();
    expect(screen.queryByText("Recents")).toBeNull();
  });
  it("opens New fleet and records account destinations", async () => {
    mount(); const user = userEvent.setup();
    await user.click(screen.getAllByRole("link", {name: "New fleet"})[0]!);
    expect(mocks.track).toHaveBeenCalledWith({source: "app_sidebar_fleets_new", surface: "app_sidebar", target: "/w/ws_1/fleets/new"});
    await openMenu();
    for (const [name, href] of [["Usage", "/settings/billing"], ["Team", "/settings/members"], ["API Keys", "/settings/security"], ["Models", "/w/ws_1/settings/models"]]) {
      expect(screen.getByRole("menuitem", {name}).getAttribute("href")).toBe(href);
    }
    expect(screen.queryByRole("menuitem", {name: "Learn more"})).toBeNull();
    await user.click(screen.getByRole("menuitem", {name: "API Keys"}));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(mocks.track).toHaveBeenCalledWith({source: "app_sidebar_settings_security", surface: "app_sidebar", target: "/settings/security"});
  });
  it("keeps fleet and resource destinations and moves settings off the canvas", () => {
    mount();
    expect(screen.getAllByRole("link", { name: "New fleet" })[0]!.getAttribute("href")).toBe("/w/ws_1/fleets/new");
    expect(screen.getByRole("link", { name: "Your fleets" }).getAttribute("aria-current")).toBe("page");
    expect(screen.queryByText("Activity")).toBeNull();
    expect(screen.queryByText("Workspace settings")).toBeNull();
    expect(screen.getByRole("button", { name: MENU }).closest("aside")).not.toBeNull();
  });
  it("keeps the approved account destinations without an extra Activity or Platform entry", async () => {
    mount(); await openMenu();
    expect(screen.queryByRole("menuitem", {name: "Platform"})).toBeNull();
    expect(screen.queryByRole("menuitem", {name: "Activity"})).toBeNull();
    expect(screen.queryByRole("menuitem", {name: "Runners"})).toBeNull();
  });
  it("keeps approval decisions inside fleets without separate Approvals or Grants destinations", async () => {
    mount(); await openMenu();
    expect(screen.getByRole("menuitem", {name: "Models"})).toBeTruthy();
    expect(screen.queryByRole("menuitem", {name: "Approvals"})).toBeNull();
    expect(screen.queryByRole("menuitem", {name: "Grants"})).toBeNull();
  });
  it("keeps all account destinations keyboard accessible in one menu", async () => {
    mount(); const menu = await openMenu();
    for (const [name, href] of [["Models", "/w/ws_1/settings/models"], ["Team", "/settings/members"], ["API Keys", "/settings/security"]])
      expect(within(menu).getByRole("menuitem", {name}).getAttribute("href")).toBe(href);
    expect(within(menu).queryByRole("menuitem", {name: "Profile"})).toBeNull();
    expect(within(menu).queryByRole("menuitem", {name: "API keys"})).toBeNull();
    await userEvent.setup().keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(screen.getByRole("button", {name: MENU}));
  });
  it("keeps collapsed destinations and the account menu reachable without a drawer", () => {
    mount();
    expect(screen.getByRole("button", {name: "Expand navigation"})).toBeTruthy();
    expect(screen.getByRole("link", {name: "Your fleets"}).textContent).toBe("");
    expect(screen.getByRole("link", { name: "Your fleets" })).toBeTruthy();
    expect(screen.getByRole("button", { name: MENU })).toBeTruthy();
    expect(screen.getByRole("link", {name: "Plugins"})).toBeTruthy();
    expect(screen.getByRole("button", {name: "Open navigation"}).getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

it("keeps account menu icons decorative", async () => {
  mount();
  await openMenu();
  for (const item of screen.getAllByRole("menuitem")) {
    expect(item.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
  }
});

it("keeps history and credits reachable when navigation is expanded, and drops stale workspace history", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({items: [], total: null, next_cursor: null}), {headers: {"Content-Type": "application/json"}})));
  const onNavigate = vi.fn();
  const view = render(<SidebarNavigation pathname="/w/ws_1/fleets" workspaceId="ws_1" collapsed={false}
    onNavigate={onNavigate} workspaces={[WORKSPACE]} billing={{balance_nanos: "3680000000", is_exhausted: false, exhausted_at: null, updated_at: "2026-11-11T00:00:00Z"}} />);
  expect(screen.getByRole("link", {name: /Credits \$3.68/}).getAttribute("href")).toBe("/settings/billing");
  expect(screen.getByText("Fleets")).toBeTruthy();
  await userEvent.setup().click(screen.getAllByRole("link", {name: "New fleet"})[0]!);
  expect(onNavigate).toHaveBeenCalledOnce();
  view.rerender(<SidebarNavigation pathname="/w/missing/fleets" workspaceId="missing" collapsed={false} onNavigate={onNavigate} workspaces={[WORKSPACE]} />);
  expect(screen.queryByText("Fleets")).toBeNull();
  expect(screen.queryByRole("link", {name: /Credits/})).toBeNull();
});

it("shows account initials, an email fallback and loading or retry status without oversized glyphs", () => {
  mocks.identity.displayName = "Kishore Neelamegam";
  const view = render(<WorkspaceSwitcherTrigger activeLabel="Main" />);
  expect(screen.getByText("KN")).toBeTruthy();
  expect(screen.getByText("Kishore Neelamegam")).toBeTruthy();
  const trigger = screen.getByRole("button");
  expect(trigger.getAttribute("aria-label")).toContain(trigger.textContent?.replace(/\s+/g, " ").trim());
  view.rerender(<WorkspaceSwitcherTrigger activeLabel="Main" collapsed />);
  expect(screen.getByRole("button").getAttribute("aria-label")).toBe("Workspace and account menu: KN");
  view.rerender(<WorkspaceSwitcherTrigger activeLabel="Main" collapsed busy />);
  expect(screen.getByRole("button").getAttribute("aria-label")).toBe("Workspace and account menu");
  mocks.identity.displayName = undefined;
  mocks.identity.emailAddress = "indy@example.test";
  view.rerender(<WorkspaceSwitcherTrigger activeLabel="Main" failed />);
  expect(screen.getByText("indy")).toBeTruthy();
  expect(screen.getByText("Retry account menu")).toBeTruthy();
  view.rerender(<WorkspaceSwitcherTrigger activeLabel="Main" busy aria-label="Workspace and account menu" />);
  expect(screen.getByRole("status").textContent).toBe("Loading workspace menu");
  mocks.identity.emailAddress = undefined;
  view.rerender(<WorkspaceSwitcherTrigger activeLabel="No workspace" />);
  expect(screen.getByText("Your workspace")).toBeTruthy();
  expect(screen.getByRole("button").querySelector("svg")?.getAttribute("width")).toBe("18");
});

it("expands and collapses the actual desktop navigation without changing its destinations", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => Response.json({items: [], next_cursor: null, total: null})));
  mount(); const user = userEvent.setup();
  await user.click(screen.getByRole("button", {name: "Expand navigation"}));
  expect(screen.getByRole("button", {name: "Collapse navigation"})).toBeTruthy();
  expect(screen.getAllByRole("link", {name: "New fleet"})[0]!.getAttribute("href")).toBe("/w/ws_1/fleets/new");
  await user.click(screen.getByRole("button", {name: "Collapse navigation"}));
  expect(screen.getByRole("button", {name: "Expand navigation"})).toBeTruthy();
});
