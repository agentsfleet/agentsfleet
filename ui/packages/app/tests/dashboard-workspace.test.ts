import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { routerPush, usePathname } from "./helpers/dashboard-mocks";
import { resetDashboardMocks, createWorkspaceActionMock } from "./helpers/dashboard-app-mocks";
import { EVENTS } from "@/lib/analytics/events";

// WorkspaceSwitcher emits the workspace-switched product event after a
// successful switch. Keep the real analytics module (its other exports are used
// transitively) and spy only on captureProductEvent.
const captureProductEventMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/analytics/posthog", async (orig) => {
  const actual = await orig<typeof import("@/lib/analytics/posthog")>();
  return { ...actual, captureProductEvent: captureProductEventMock };
});

// Common dashboard mock harness — see tests/helpers/dashboard-mocks.tsx.
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@clerk/nextjs", async () => (await import("./helpers/dashboard-mocks")).clerkMock());
vi.mock("@clerk/nextjs/server", async () => (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("@/lib/workspace", async () => (await import("./helpers/dashboard-mocks")).workspaceMock());
vi.mock("lucide-react", async () => (await import("./helpers/dashboard-mocks")).lucideMock());
vi.mock("@agentsfleet/design-system", async (orig) => {
  const h = await import("./helpers/dashboard-mocks");
  return { ...h.designSystemCore(await orig<Record<string, unknown>>()), ...h.designSystemDropdown() };
});

// App-specific dashboard mocks — see tests/helpers/dashboard-app-mocks.tsx.
vi.mock("@/lib/api/fleets", async () => (await import("./helpers/dashboard-app-mocks")).fleetsApiMock());
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", async () => (await import("./helpers/dashboard-app-mocks")).fleetActionsMock());
vi.mock("@/lib/api/tenant_billing", async () => (await import("./helpers/dashboard-app-mocks")).tenantBillingMock());
vi.mock("@/lib/api/tenant_provider", async () => (await import("./helpers/dashboard-app-mocks")).tenantProviderMock());
vi.mock("@/app/(dashboard)/settings/billing/components/BillingBalanceCard", async () => (await import("./helpers/dashboard-app-mocks")).billingBalanceCardMock());
vi.mock("@/app/(dashboard)/settings/billing/components/BillingUsageTab", async () => (await import("./helpers/dashboard-app-mocks")).billingUsageTabMock());
vi.mock("@/lib/api/events", async () => (await import("./helpers/dashboard-app-mocks")).eventsMock());
vi.mock("@/lib/api/secrets", async () => (await import("./helpers/dashboard-app-mocks")).secretsApiMock());
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/components/AddSecretForm", async () => (await import("./helpers/dashboard-app-mocks")).addSecretFormMock());
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList", async () => (await import("./helpers/dashboard-app-mocks")).secretsListMock());
vi.mock("@/app/(dashboard)/actions", async () => (await import("./helpers/dashboard-app-mocks")).dashboardActionsMock());

beforeEach(() => {
  vi.clearAllMocks();
  resetDashboardMocks();
});
afterEach(() => {
  cleanup();
});

describe("WorkspaceSwitcher component", () => {
  async function renderSwitcher(props: {
    workspaces?: Array<{ id: string; name: string | null }>;
    activeId?: string | null;
  } = {}) {
    const { default: WorkspaceSwitcher } = await import(
      "../components/layout/WorkspaceSwitcher"
    );
    render(
      React.createElement(WorkspaceSwitcher, {
        workspaces: props.workspaces ?? [
          { id: "ws_1", name: "Alpha" },
          { id: "ws_2", name: "Beta" },
        ],
        activeId: props.activeId ?? "ws_1",
      } as never),
    );
  }

  it("still renders with a Create workspace affordance when workspaces is empty", async () => {
    render(
      React.createElement(
        (await import("../components/layout/WorkspaceSwitcher")).default,
        { workspaces: [], activeId: null } as never,
      ),
    );
    // The empty case is exactly when create matters most — switcher must show.
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain("No workspace");
    expect(screen.getByTestId("workspace-new")).toBeTruthy();
  });

  it("opens the create dialog from the Create workspace item", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitcher();
    await user.click(screen.getByTestId("workspace-new"));
    await waitFor(() => expect(screen.getByTestId("workspace-name-input")).toBeTruthy());
  });

  it("bounds the workspace menu so create actions remain reachable", async () => {
    const manyWorkspaces = Array.from({ length: 32 }, (_, index) => ({
      id: `ws_${index}`,
      name: `Workspace ${index}`,
    }));
    const { container } = render(
      React.createElement(
        (await import("../components/layout/WorkspaceSwitcher")).default,
        {
          workspaces: manyWorkspaces,
          activeId: "ws_0",
        } as never,
      ),
    );
    const menu = container.querySelector("[data-dropdown-content]");
    expect(menu?.className).toContain("max-h-96");
    expect(menu?.className).toContain("overflow-y-auto");
  });

  it("renders the active workspace label", async () => {
    await renderSwitcher();
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain("Alpha");
  });

  it("falls back to id when name is null", async () => {
    await renderSwitcher({
      workspaces: [{ id: "ws_only", name: null }],
      activeId: "ws_only",
    });
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain("ws_only");
  });

  it("falls back to the first workspace when activeId is unknown", async () => {
    await renderSwitcher({
      workspaces: [
        { id: "ws_a", name: "Alpha" },
        { id: "ws_b", name: "Beta" },
      ],
      activeId: "ws_unknown",
    });
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain("Alpha");
  });

  it("picking a different workspace navigates to its URL (same sub-path), writes no cookie", async () => {
    const user = userEvent.setup({ delay: null });
    // Switching preserves the current sub-page: /w/ws_1/fleets → /w/ws_2/fleets.
    usePathname.mockReturnValue("/w/ws_1/fleets");
    await renderSwitcher();
    const items = screen.getAllByRole("menuitem");
    // Second item = Beta (different from active ws_1)
    await user.click(items[1]!);
    await waitFor(() =>
      expect(routerPush).toHaveBeenCalledWith("/w/ws_2/fleets"),
    );
    // The product event fires with the picked workspace id after the switch.
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.workspace_switched, { workspace_id: "ws_2" });
    await waitFor(() =>
      expect(screen.getByText("Workspace changed to Beta.")).toBeTruthy(),
    );
  });

  it("collapses a resource-detail path to its section on switch (avoids a guaranteed 404)", async () => {
    const user = userEvent.setup({ delay: null });
    // On /w/ws_1/fleets/fleet_abc the fleet id belongs to ws_1; switching to ws_2
    // lands on ws_2's fleets list, not /w/ws_2/fleets/fleet_abc (which would 404).
    usePathname.mockReturnValue("/w/ws_1/fleets/fleet_abc");
    await renderSwitcher();
    const items = screen.getAllByRole("menuitem");
    await user.click(items[1]!);
    await waitFor(() => expect(routerPush).toHaveBeenCalledWith("/w/ws_2/fleets"));
  });

  it("navigates into the displayed workspace from a tenant page (activeId is only a display fallback)", async () => {
    const user = userEvent.setup({ delay: null });
    // On /settings/billing there is no /w/ segment, so `activeId` is the fallback
    // first workspace. Picking it must still navigate (not no-op) into its home.
    usePathname.mockReturnValue("/settings/billing");
    await renderSwitcher({ activeId: "ws_1" });
    const items = screen.getAllByRole("menuitem");
    // First item = Alpha (the displayed-active fallback)
    await user.click(items[0]!);
    await waitFor(() => expect(routerPush).toHaveBeenCalledWith("/w/ws_1"));
  });

  it("uses the workspace id in the switch toast when the workspace has no name", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitcher({
      workspaces: [
        { id: "ws_1", name: "Alpha" },
        { id: "ws_no_name", name: null },
      ],
    });
    const items = screen.getAllByRole("menuitem");
    await user.click(items[1]!);
    await waitFor(() =>
      expect(screen.getByText("Workspace changed to ws_no_name.")).toBeTruthy(),
    );
  });

  it("clears the workspace switch toast after the notice timeout", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitcher();
    const items = screen.getAllByRole("menuitem");
    await user.click(items[1]!);
    await waitFor(() =>
      expect(screen.getByText("Workspace changed to Beta.")).toBeTruthy(),
    );
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 2900));
    });
    await waitFor(() =>
      expect(screen.queryByText("Workspace changed to Beta.")).toBeNull(),
    );
  });

  it("picking the active workspace is a no-op", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitcher();
    const items = screen.getAllByRole("menuitem");
    // First item = Alpha (same as active)
    await user.click(items[0]!);
    // Give transition a tick
    await new Promise((r) => setTimeout(r, 10));
    expect(routerPush).not.toHaveBeenCalled();
    expect(captureProductEventMock).not.toHaveBeenCalled();
  });

  it("has no Manage workspace item — switching/creating are the only actions", async () => {
    // The dropdown menu is entirely switch-or-create; there's nothing left to
    // "manage" once workspace identity moved to the API Keys page.
    await renderSwitcher();
    expect(screen.queryByTestId("workspace-manage")).toBeNull();
  });

  it("creates a workspace from the dropdown item and shows the created toast", async () => {
    const user = userEvent.setup({ delay: null });
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_inline", name: "inline-prod" },
    });
    await renderSwitcher();
    await user.click(screen.getByTestId("workspace-new"));
    // findByTestId, not getByTestId — the input mounts on the dropdown item's
    // state flip, and a sync query races that render under a loaded shuffle run.
    await user.type(await screen.findByTestId("workspace-name-input"), "inline-prod");
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(screen.getByText("Workspace created: inline-prod.")).toBeTruthy(),
    );
    expect(screen.getByRole("menuitem", { name: "inline-prod" })).toBeTruthy();
  });
});
