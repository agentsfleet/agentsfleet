import React from "react";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { OWN_ACCOUNT } from "./helpers/workspace-fixtures";

const { refresh, route, chunk } = vi.hoisted(() => ({ refresh: vi.fn(), route: { path: "/w/new/fleets/new" },
  chunk: {status: "idle", preload: vi.fn(), retry: vi.fn()} }));
vi.mock("next/navigation", () => ({ usePathname: () => route.path, useRouter: () => ({ refresh }) }));
vi.mock("@/components/layout/WorkspaceCreationProvider", () => ({ useCreatedWorkspaces: () => [] }));
vi.mock("@/components/domain/island-dynamic/intent-module-loader", () => ({
  createIntentModuleLoader: () => chunk, INTENT_MODULE_STATUS: { error: "error", loading: "loading" },
  useIntentModule: () => chunk, maySpeculateOnHover: () => false,
}));
import WorkspaceSwitcher from "@/components/layout/WorkspaceSwitcher";
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => ({isLoaded: true, isSignedIn: true, userId: "model-owner"})}));

afterEach(() => { cleanup(); vi.clearAllMocks(); route.path = "/w/new/fleets/new"; chunk.status = "idle"; });
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

it("offers explicit retry after the account-menu chunk fails to load", async () => {
  chunk.status = "error";
  render(<WorkspaceSwitcher workspaces={[]} />);
  await userEvent.setup().click(screen.getByRole("button", {name: /^Retry account menu(?::|$)/}));
  expect(chunk.retry).toHaveBeenCalledOnce();
  expect(screen.getByRole("button", {name: /^Retry account menu(?::|$)/}).getAttribute("aria-busy")).toBe("false");
});
