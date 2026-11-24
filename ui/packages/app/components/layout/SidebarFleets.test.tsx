import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { FLEET_DETAIL, WORKSPACE, response } from "@/tests/helpers/fleet-fixtures";
import { SidebarFleets } from "./SidebarFleets";

const {identity} = vi.hoisted(() => ({identity: {userId: "reader" as string | null}}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => identity}));
vi.mock("next/navigation", () => ({usePathname: () => `/w/${WORKSPACE}/fleets/new`, useRouter: () => ({replace: vi.fn(), refresh: vi.fn()})}));
afterEach(() => {cleanup(); vi.restoreAllMocks(); identity.userId = "reader";});

it.each([false, true])("lists the workspace's fleets under Your Fleets (overlay %s)", async overlay => {
  const fetcher = vi.spyOn(globalThis, "fetch").mockImplementation(async () => response({items: [FLEET_DETAIL], total: null, next_cursor: null}));
  const view = render(<SidebarFleets account="account" workspace={WORKSPACE} overlay={overlay} onNavigate={() => {}} />);
  expect(screen.getByText("Your Fleets")).toBeTruthy();
  expect(await screen.findByRole("link", {name: FLEET_DETAIL.name})).toBeTruthy();
  expect(view.container.firstElementChild?.classList.contains("md:px-lg")).toBe(!overlay);
  await waitFor(() => expect(fetcher).toHaveBeenCalledOnce());
});

it.each([
  {account: null, workspace: WORKSPACE, user: "reader"},
  {account: "account", workspace: null, user: "reader"},
  {account: "account", workspace: WORKSPACE, user: null},
])("reads no fleets without an account, a workspace and a signed-in person (%o)", ({account, workspace, user}) => {
  identity.userId = user;
  const fetcher = vi.spyOn(globalThis, "fetch");
  render(<SidebarFleets account={account} workspace={workspace} onNavigate={vi.fn()} />);
  expect(screen.queryByText("Your Fleets")).toBeNull();
  expect(fetcher).not.toHaveBeenCalled();
});
