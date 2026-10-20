import React from "react";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { OWN_ACCOUNT } from "./helpers/workspace-fixtures";

const { refresh, route } = vi.hoisted(() => ({ refresh: vi.fn(), route: { path: "/w/new/fleets/new" } }));
vi.mock("next/navigation", () => ({ usePathname: () => route.path, useRouter: () => ({ refresh }) }));
vi.mock("@/components/layout/WorkspaceCreationProvider", () => ({ useCreatedWorkspaces: () => [] }));
vi.mock("@/components/layout/WorkspaceSwitcherTrigger", () => ({ WorkspaceSwitcherTrigger: () => null }));
vi.mock("@/components/domain/island-dynamic/intent-module-loader", () => ({
  createIntentModuleLoader: () => ({}), INTENT_MODULE_STATUS: { error: "error", loading: "loading" },
  useIntentModule: () => ({ status: "idle" }), maySpeculateOnHover: () => false,
}));
import WorkspaceSwitcher from "@/components/layout/WorkspaceSwitcher";

afterEach(() => { cleanup(); vi.clearAllMocks(); route.path = "/w/new/fleets/new"; });
it("refreshes stale shell workspace data once per missing workspace without a refresh loop", () => {
  const view = render(<WorkspaceSwitcher workspaces={[]} />);
  expect(refresh).toHaveBeenCalledTimes(1);
  view.rerender(<WorkspaceSwitcher workspaces={[]} />);
  expect(refresh).toHaveBeenCalledTimes(1);
  route.path = "/w/other/fleets/new";
  view.rerender(<WorkspaceSwitcher workspaces={[]} />);
  expect(refresh).toHaveBeenCalledTimes(2);
});
it("does not refresh a resolved workspace or an account page", () => {
  const workspaces = [{ id: "new", name: "New workspace", created_at: "2026-10-19T00:00:00Z", ...OWN_ACCOUNT }];
  const view = render(<WorkspaceSwitcher workspaces={workspaces} />);
  route.path = "/settings/account";
  view.rerender(<WorkspaceSwitcher workspaces={workspaces} />);
  expect(refresh).not.toHaveBeenCalled();
});
