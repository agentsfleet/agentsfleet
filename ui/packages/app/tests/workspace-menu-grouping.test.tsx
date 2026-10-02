import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const { pathname, push } = vi.hoisted(() => ({ pathname: vi.fn(), push: vi.fn() }));
vi.mock("next/navigation", () => ({
  usePathname: pathname,
  useRouter: () => ({ push }),
}));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: vi.fn() }));
vi.mock("@/components/domain/island-dynamic/CreateWorkspaceDialogDynamic", () => ({ default: () => null }));
vi.mock("@/components/layout/WorkspaceCreationProvider", () => ({
  useWorkspaceCreation: () => ({
    createdWorkspaces: [], locked: false, pending: false, error: null,
    reset: vi.fn(), dismiss: vi.fn(), create: vi.fn(), showNotice: vi.fn(),
  }),
}));

import WorkspaceSwitcherMenu from "@/components/layout/WorkspaceSwitcherMenu";
import { accountLabel, OWN_ACCOUNT_LABEL } from "@/components/layout/workspace-groups";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces-types";

const MINE = { tenant_id: "tenant_bob", owner_name: "Bob" };
const JOHNS = { tenant_id: "tenant_john", owner_name: "John" };
// A second account whose owner is also called John.
const OTHER_JOHNS = { tenant_id: "tenant_john_2", owner_name: JOHNS.owner_name };
const BOB_HOME: TenantWorkspace = { id: "ws_bob", name: "bob-home", created_at: 1, account: MINE, role: ACCOUNT_ROLE.owner };
const MARY_001: TenantWorkspace = { id: "ws_john", name: "mary-001", created_at: 2, account: JOHNS, role: ACCOUNT_ROLE.member };
const OPS_001: TenantWorkspace = { id: "ws_john_2", name: "ops-001", created_at: 3, account: OTHER_JOHNS, role: ACCOUNT_ROLE.member };
const DUPLICATE_KEY = /same key/;

beforeEach(() => { pathname.mockReturnValue(`/w/${BOB_HOME.id}/fleets`); });
afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("workspace menu grouped by account", () => {
  it("should show Bob's own workspace under Yours and John's under John's account", () => {
    render(<WorkspaceSwitcherMenu open workspaces={[MARY_001, BOB_HOME]} onOpenChange={vi.fn()} />);
    const menu = screen.getByRole("menu");
    const labels = within(menu).getAllByText(new RegExp(`^(${OWN_ACCOUNT_LABEL}|${accountLabel(JOHNS.owner_name)})$`));
    expect(labels.map((label) => label.textContent)).toEqual([OWN_ACCOUNT_LABEL, accountLabel(JOHNS.owner_name)]);
    const items = within(menu).getAllByRole("menuitem").map((item) => item.textContent ?? "");
    expect(items.findIndex((text) => text.includes("bob-home"))).toBeLessThan(items.findIndex((text) => text.includes("mary-001")));
  });

  it("should set an account's name in sentence case under the menu's eyebrow heading", () => {
    render(<WorkspaceSwitcherMenu open workspaces={[MARY_001, BOB_HOME]} onOpenChange={vi.fn()} />);
    const menu = screen.getByRole("menu");
    const account = within(menu).getByText(accountLabel(JOHNS.owner_name)).className.split(" ");
    expect(account).toEqual(expect.arrayContaining(["text-label", "leading-label", "tracking-label"]));
    expect(account).not.toContain("uppercase");
    expect(within(menu).getByText("Workspace").className.split(" ")).toContain("uppercase");
  });

  it("should label nothing while the person holds only their own account", () => {
    render(<WorkspaceSwitcherMenu open workspaces={[BOB_HOME]} onOpenChange={vi.fn()} />);
    expect(screen.queryByText(OWN_ACCOUNT_LABEL)).toBeNull();
    expect(screen.queryByRole("group")).toBeNull();
    expect(screen.getByRole("menuitem", { name: /bob-home/ })).toBeTruthy();
  });

  it("should group each account's workspaces under its label for screen readers", () => {
    render(<WorkspaceSwitcherMenu open workspaces={[MARY_001, BOB_HOME]} onOpenChange={vi.fn()} />);
    const yours = screen.getByRole("group", { name: OWN_ACCOUNT_LABEL });
    const johns = screen.getByRole("group", { name: accountLabel(JOHNS.owner_name) });
    expect(within(yours).getByRole("menuitem", { name: /bob-home/ })).toBeTruthy();
    expect(within(johns).getByRole("menuitem", { name: /mary-001/ })).toBeTruthy();
    expect(within(johns).queryByRole("menuitem", { name: /bob-home/ })).toBeNull();
  });

  it("should list both workspaces of two accounts whose owners share a name, with no duplicate key", () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    render(<WorkspaceSwitcherMenu open workspaces={[BOB_HOME, MARY_001, OPS_001]} onOpenChange={vi.fn()} />);
    expect(screen.getByRole("menuitem", { name: /mary-001/ })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: /ops-001/ })).toBeTruthy();
    expect(screen.getAllByRole("group", { name: accountLabel(JOHNS.owner_name) })).toHaveLength(2);
    expect(consoleError.mock.calls.flat().join(" ")).not.toMatch(DUPLICATE_KEY);
    consoleError.mockRestore();
  });

  it("should navigate to a joined account's workspace when it is picked", async () => {
    render(<WorkspaceSwitcherMenu open workspaces={[BOB_HOME, MARY_001]} onOpenChange={vi.fn()} />);
    await userEvent.setup().click(screen.getByRole("menuitem", { name: /mary-001/ }));
    expect(push).toHaveBeenCalledWith(`/w/${MARY_001.id}/fleets`);
  });
});
