import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { routerRefresh } from "./helpers/dashboard-mocks";
import { resetDashboardMocks, stopZombieMock, setZombieStatusActionMock } from "./helpers/dashboard-app-mocks";

// Common dashboard mock harness — see tests/helpers/dashboard-mocks.tsx.
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@clerk/nextjs", async () => (await import("./helpers/dashboard-mocks")).clerkMock());
vi.mock("@clerk/nextjs/server", async () => (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("@/lib/workspace", async () => (await import("./helpers/dashboard-mocks")).workspaceMock());
vi.mock("lucide-react", async () => (await import("./helpers/dashboard-mocks")).lucideMock());
vi.mock("@usezombie/design-system", async (orig) => {
  const h = await import("./helpers/dashboard-mocks");
  return { ...h.designSystemCore(await orig<Record<string, unknown>>()), ...h.designSystemDropdown() };
});

// App-specific dashboard mocks — see tests/helpers/dashboard-app-mocks.tsx.
vi.mock("@/lib/api/zombies", async () => (await import("./helpers/dashboard-app-mocks")).zombiesApiMock());
vi.mock("@/app/(dashboard)/zombies/actions", async () => (await import("./helpers/dashboard-app-mocks")).zombieActionsMock());
vi.mock("@/lib/api/tenant_billing", async () => (await import("./helpers/dashboard-app-mocks")).tenantBillingMock());
vi.mock("@/lib/api/tenant_provider", async () => (await import("./helpers/dashboard-app-mocks")).tenantProviderMock());
vi.mock("@/app/(dashboard)/settings/models/components/ProviderSelector", async () => (await import("./helpers/dashboard-app-mocks")).providerSelectorMock());
vi.mock("@/app/(dashboard)/settings/billing/components/BillingBalanceCard", async () => (await import("./helpers/dashboard-app-mocks")).billingBalanceCardMock());
vi.mock("@/app/(dashboard)/settings/billing/components/BillingUsageTab", async () => (await import("./helpers/dashboard-app-mocks")).billingUsageTabMock());
vi.mock("@/lib/api/events", async () => (await import("./helpers/dashboard-app-mocks")).eventsMock());
vi.mock("@/lib/api/credentials", async () => (await import("./helpers/dashboard-app-mocks")).credentialsApiMock());
vi.mock("@/app/(dashboard)/credentials/components/AddCredentialForm", async () => (await import("./helpers/dashboard-app-mocks")).addCredentialFormMock());
vi.mock("@/app/(dashboard)/credentials/components/CredentialsList", async () => (await import("./helpers/dashboard-app-mocks")).credentialsListMock());
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
      "../app/(dashboard)/zombies/[id]/components/KillSwitch"
    );
    render(
      React.createElement(KillSwitch, {
        workspaceId: "ws_1",
        zombie: { id: "zom_1", name: "alpha", status, created_at: "2026-04-22T00:00:00Z" },
      } as never),
    );
  }

  it("renders Killed label when zombie is terminal (no actions)", async () => {
    await renderSwitch("killed");
    expect(screen.getByText("Killed")).toBeTruthy();
  });

  it("offers Resume + Kill when zombie is stopped", async () => {
    await renderSwitch("stopped");
    expect(screen.getByRole("button", { name: /^resume$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^kill$/i })).toBeTruthy();
  });

  it("offers Resume + Kill when zombie is paused (auto-halt)", async () => {
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

  it("active → Stop happy path: click → confirm → setZombieStatusAction(stopped) → refresh", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() =>
      expect(setZombieStatusActionMock).toHaveBeenCalledWith("ws_1", "zom_1", "stopped"),
    );
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("stopped → Resume sends status='active'", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitch("stopped");
    await user.click(screen.getByRole("button", { name: /^resume$/i }));
    await clickConfirmInDialog(user, /^resume$/i);
    await waitFor(() =>
      expect(setZombieStatusActionMock).toHaveBeenCalledWith("ws_1", "zom_1", "active"),
    );
  });

  it("active → Kill sends status='killed'", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^kill$/i }));
    await clickConfirmInDialog(user, /^kill$/i);
    await waitFor(() =>
      expect(setZombieStatusActionMock).toHaveBeenCalledWith("ws_1", "zom_1", "killed"),
    );
  });

  it("409 conflict closes the dialog and refreshes (status changed elsewhere)", async () => {
    const { ApiError } = await import("../lib/api/errors");
    stopZombieMock.mockRejectedValue(new ApiError("transition not allowed", 409, "UZ-ZMB-010", "req_x"));
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("non-409 error keeps dialog open (status rolled back)", async () => {
    stopZombieMock.mockRejectedValue(new Error("network down"));
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() => expect(stopZombieMock).toHaveBeenCalled());
    expect(screen.queryByRole("alertdialog")).toBeTruthy();
  });

  it("server action reporting unauth surfaces the error and rolls back the optimistic flip", async () => {
    setZombieStatusActionMock.mockResolvedValueOnce({
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
    expect(stopZombieMock).not.toHaveBeenCalled();
  });

  it("server action returning empty error string falls back to 'Failed to stop agent' default", async () => {
    setZombieStatusActionMock.mockResolvedValueOnce({
      ok: false,
      error: "",
      status: 500,
    });
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't stop this agent/i),
    );
  });

  // WS-G — every ActionConfig carries its own `errorVerb` literal so the
  // operator-facing sentence reads naturally per action. The Stop case above
  // exercises the Stop verb; the next two pin Resume and Kill so each branch
  // of the static-literal config is hit by patch coverage.
  it("resume action error path renders 'Couldn't resume this agent' (WS-G verb literal)", async () => {
    setZombieStatusActionMock.mockResolvedValueOnce({
      ok: false,
      error: "",
      status: 500,
    });
    const user = userEvent.setup({ delay: null });
    await renderSwitch("stopped");
    await user.click(screen.getByRole("button", { name: /^resume$/i }));
    await clickConfirmInDialog(user, /^resume$/i);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't resume this agent/i),
    );
  });

  it("kill action error path renders 'Couldn't kill this agent' (WS-G verb literal)", async () => {
    setZombieStatusActionMock.mockResolvedValueOnce({
      ok: false,
      error: "",
      status: 500,
    });
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^kill$/i }));
    await clickConfirmInDialog(user, /^kill$/i);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't kill this agent/i),
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
    expect(setZombieStatusActionMock).not.toHaveBeenCalled();
  });
});
