import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { routerRefresh } from "./helpers/dashboard-mocks";
import { resetDashboardMocks, stopFleetMock, setFleetStatusActionMock } from "./helpers/dashboard-app-mocks";

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
vi.mock("@/app/(dashboard)/fleets/actions", async () => (await import("./helpers/dashboard-app-mocks")).fleetActionsMock());
vi.mock("@/lib/api/tenant_billing", async () => (await import("./helpers/dashboard-app-mocks")).tenantBillingMock());
vi.mock("@/lib/api/tenant_provider", async () => (await import("./helpers/dashboard-app-mocks")).tenantProviderMock());
vi.mock("@/app/(dashboard)/settings/billing/components/BillingBalanceCard", async () => (await import("./helpers/dashboard-app-mocks")).billingBalanceCardMock());
vi.mock("@/app/(dashboard)/settings/billing/components/BillingUsageTab", async () => (await import("./helpers/dashboard-app-mocks")).billingUsageTabMock());
vi.mock("@/lib/api/events", async () => (await import("./helpers/dashboard-app-mocks")).eventsMock());
vi.mock("@/lib/api/secrets", async () => (await import("./helpers/dashboard-app-mocks")).secretsApiMock());
vi.mock("@/app/(dashboard)/secrets/components/AddSecretForm", async () => (await import("./helpers/dashboard-app-mocks")).addSecretFormMock());
vi.mock("@/app/(dashboard)/secrets/components/SecretsList", async () => (await import("./helpers/dashboard-app-mocks")).secretsListMock());
vi.mock("@/app/(dashboard)/actions", async () => (await import("./helpers/dashboard-app-mocks")).dashboardActionsMock());

beforeEach(() => {
  vi.clearAllMocks();
  resetDashboardMocks();
});
afterEach(() => {
  cleanup();
});

describe("KillSwitch component", () => {
  async function renderSwitch(status: string = "active") {
    const { default: KillSwitch } = await import(
      "../app/(dashboard)/fleets/[id]/components/KillSwitch"
    );
    render(
      React.createElement(KillSwitch, {
        workspaceId: "ws_1",
        fleet: { id: "zom_1", name: "alpha", status, created_at: "2026-04-22T00:00:00Z" },
      } as never),
    );
  }

  it("renders Killed label when fleet is terminal (no actions)", async () => {
    await renderSwitch("killed");
    expect(screen.getByText("Killed")).toBeTruthy();
  });

  it("offers Resume + Kill when fleet is stopped", async () => {
    await renderSwitch("stopped");
    expect(screen.getByRole("button", { name: /^resume$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^kill$/i })).toBeTruthy();
  });

  it("offers Resume + Kill when fleet is paused (auto-halt)", async () => {
    await renderSwitch("paused");
    expect(screen.getByRole("button", { name: /^resume$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^kill$/i })).toBeTruthy();
  });

  // After opening the action dialog, both the trigger button and the
  // ConfirmDialog confirm button carry the same accessible name. Scope the
  // confirm-click to the alertdialog subtree to disambiguate.
  async function clickConfirmInDialog(user: ReturnType<typeof userEvent.setup>, name: RegExp) {
    const dialog = await screen.findByRole("alertdialog");
    const { within } = await import("@testing-library/react");
    await user.click(within(dialog).getByRole("button", { name }));
  }

  it("active → Stop happy path: click → confirm → setFleetStatusAction(stopped) → refresh", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() =>
      expect(setFleetStatusActionMock).toHaveBeenCalledWith("ws_1", "zom_1", "stopped"),
    );
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("stopped → Resume sends status='active'", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitch("stopped");
    await user.click(screen.getByRole("button", { name: /^resume$/i }));
    await clickConfirmInDialog(user, /^resume$/i);
    await waitFor(() =>
      expect(setFleetStatusActionMock).toHaveBeenCalledWith("ws_1", "zom_1", "active"),
    );
  });

  it("active → Kill sends status='killed'", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^kill$/i }));
    await clickConfirmInDialog(user, /^kill$/i);
    await waitFor(() =>
      expect(setFleetStatusActionMock).toHaveBeenCalledWith("ws_1", "zom_1", "killed"),
    );
  });

  it("409 conflict closes the dialog and refreshes (status changed elsewhere)", async () => {
    const { ApiError } = await import("../lib/api/errors");
    stopFleetMock.mockRejectedValue(new ApiError("transition not allowed", 409, "UZ-AGT-010", "req_x"));
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("non-409 error keeps dialog open (status rolled back)", async () => {
    stopFleetMock.mockRejectedValue(new Error("network down"));
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() => expect(stopFleetMock).toHaveBeenCalled());
    expect(screen.queryByRole("alertdialog")).toBeTruthy();
  });

  it("server action reporting unauth surfaces the error and rolls back the optimistic flip", async () => {
    setFleetStatusActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Not authenticated",
      status: 401,
    });
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Not authenticated/),
    );
    expect(stopFleetMock).not.toHaveBeenCalled();
  });

  it("server action returning empty error string falls back to 'Failed to stop fleet' default", async () => {
    setFleetStatusActionMock.mockResolvedValueOnce({
      ok: false,
      error: "",
      status: 500,
    });
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't stop this fleet/i),
    );
  });

  // WS-G — every ActionConfig carries its own `errorVerb` literal so the
  // operator-facing sentence reads naturally per action. The Stop case above
  // exercises the Stop verb; the next two pin Resume and Kill so each branch
  // of the static-literal config is hit by patch coverage.
  it("resume action error path renders 'Couldn't resume this fleet' (WS-G verb literal)", async () => {
    setFleetStatusActionMock.mockResolvedValueOnce({
      ok: false,
      error: "",
      status: 500,
    });
    const user = userEvent.setup({ delay: null });
    await renderSwitch("stopped");
    await user.click(screen.getByRole("button", { name: /^resume$/i }));
    await clickConfirmInDialog(user, /^resume$/i);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't resume this fleet/i),
    );
  });

  it("kill action error path renders 'Couldn't kill this fleet' (WS-G verb literal)", async () => {
    setFleetStatusActionMock.mockResolvedValueOnce({
      ok: false,
      error: "",
      status: 500,
    });
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^kill$/i }));
    await clickConfirmInDialog(user, /^kill$/i);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't kill this fleet/i),
    );
  });

  // Pins the dialog-dismiss path: clicking Cancel drives onOpenChange(false)
  // which clears pendingAction. Without this, the close-handler line stays
  // uncovered by patch coverage even though every other interaction works.
  it("Cancel dismisses the confirm dialog and clears pendingAction", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    const dialog = await screen.findByRole("alertdialog");
    const { within } = await import("@testing-library/react");
    await user.click(within(dialog).getByRole("button", { name: /cancel/i }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(setFleetStatusActionMock).not.toHaveBeenCalled();
  });
});
