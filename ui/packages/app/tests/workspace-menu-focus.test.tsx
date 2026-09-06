import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const { pathname } = vi.hoisted(() => ({ pathname: vi.fn() }));
vi.mock("next/navigation", () => ({
  usePathname: pathname,
  useRouter: () => ({ push: vi.fn() }),
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

beforeEach(() => { pathname.mockReturnValue("/w/ws_0/fleets"); });
afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("lazy workspace menu entry focus", () => {
  it("focuses a workspace when mounted open and keeps arrow-key navigation working", async () => {
    const workspaces = Array.from({ length: 32 }, (_, index) => ({
      id: `ws_${index}`, name: `Workspace ${index}`, created_at: 1,
    }));
    render(<WorkspaceSwitcherMenu open workspaces={workspaces} onOpenChange={vi.fn()} />);
    const first = await screen.findByRole("menuitem", { name: "Workspace 0" });
    await waitFor(() => expect(document.activeElement).toBe(first));
    expect(first.tabIndex).toBe(0);
    expect(screen.getByTestId("workspace-list-scroll").contains(first)).toBe(true);
    const user = userEvent.setup();
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Workspace 1" }));
    await user.keyboard("{End}");
    expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Create workspace" }));
    await user.keyboard("{Home}");
    expect(document.activeElement).toBe(first);
  });

  it("focuses Create workspace when the account has no workspace to select", async () => {
    pathname.mockReturnValue("/settings/api-keys");
    render(<WorkspaceSwitcherMenu open workspaces={[]} onOpenChange={vi.fn()} />);
    const create = await screen.findByRole("menuitem", { name: "Create workspace" });
    await waitFor(() => expect(document.activeElement).toBe(create));
    expect(create.tabIndex).toBe(0);
  });
});
