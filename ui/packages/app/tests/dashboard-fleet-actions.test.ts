import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { routerPush, routerRefresh } from "./helpers/dashboard-mocks";
import { deleteFleetActionMock, resetDashboardMocks, setFleetStatusMock, setFleetStatusActionMock } from "./helpers/dashboard-app-mocks";

const ARCHIVE_FAILURE_PATTERN = /Couldn't archive this fleet/i;

// Common dashboard mock harness — see tests/helpers/dashboard-mocks.tsx.
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@clerk/nextjs", async () => (await import("./helpers/dashboard-mocks")).clerkMock());
vi.mock("@clerk/nextjs/server", async () => (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("@/lib/workspace", async () => (await import("./helpers/dashboard-mocks")).workspaceMock());
vi.mock("lucide-react", async () => (await import("./helpers/dashboard-mocks")).lucideMock());
vi.mock("@agentsfleet/design-system", async (orig) => {
  const h = await import("./helpers/dashboard-mocks");
  return h.designSystemCore(await orig<Record<string, unknown>>());
});

// App-specific dashboard mocks — see tests/helpers/dashboard-app-mocks.tsx.
vi.mock("@/lib/api/fleets", async () => (await import("./helpers/dashboard-app-mocks")).fleetsApiMock());
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", async () => (await import("./helpers/dashboard-app-mocks")).fleetActionsMock());
vi.mock("@/lib/api/tenant_billing", async () => (await import("./helpers/dashboard-app-mocks")).tenantBillingMock());
vi.mock("@/lib/api/tenant_provider", async () => (await import("./helpers/dashboard-app-mocks")).tenantProviderMock());
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

describe("FleetActions component", () => {
  async function renderSwitch(status: string = "active") {
    const { default: FleetActions } = await import(
      "../app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetActions"
    );
    render(
      React.createElement(FleetActions, {
        workspaceId: "ws_1",
        fleet: { id: "zom_1", name: "Pull request review", status, etag: '"displayed-fleet"' },
      }),
    );
  }

  async function openAction(user: ReturnType<typeof userEvent.setup>, name: RegExp) {
    await user.click(screen.getByRole("button", {name: "Fleet actions"}));
    await user.click(screen.getByRole("menuitem", {name}));
  }

  // Scope confirmation to the dialog so a menu action can never bypass it.
  async function clickConfirmInDialog(user: ReturnType<typeof userEvent.setup>, name: RegExp) {
    const dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name }));
  }

  it("offers only Delete once the fleet is archived", async () => {
    await renderSwitch("archived");
    await userEvent.click(screen.getByRole("button", {name: "Fleet actions"}));
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Delete fleet"]);
    expect(setFleetStatusActionMock).not.toHaveBeenCalled();
  });

  it.each(["active", "installing"])("offers Archive + Delete while %s, and no Stop, Resume or Kill", async (status) => {
    await renderSwitch(status);
    await userEvent.click(screen.getByRole("button", {name: "Fleet actions"}));
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Archive", "Delete fleet"]);
    expect(screen.queryByRole("menuitem", {name: /^(stop|resume|kill)$/i})).toBeNull();
  });

  it("keeps the terminal actions in an accessible menu and retains their confirmation", async () => {
    await renderSwitch("active");
    expect(screen.queryByRole("button", {name: /^archive$/i})).toBeNull();
    expect(screen.queryByRole("button", {name: /^delete fleet$/i})).toBeNull();
    const trigger = screen.getByRole("button", {name: "Fleet actions"});
    expect(trigger.querySelector("[data-icon]")?.getAttribute("aria-hidden")).toBe("true");
    await userEvent.click(trigger);
    expect(screen.getByRole("menuitem", {name: /^archive$/i})).toBeTruthy();
    expect(setFleetStatusActionMock).not.toHaveBeenCalled();
    expect(deleteFleetActionMock).not.toHaveBeenCalled();
  });

  it("active → Archive happy path: confirm → setFleetStatusAction(archived) → refresh", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await openAction(user, /^archive$/i);
    expect(screen.getByRole("alertdialog").textContent).toMatch(/keeps its chat and activity/i);
    await clickConfirmInDialog(user, /^archive$/i);
    await waitFor(() =>
      expect(setFleetStatusActionMock).toHaveBeenCalledWith("ws_1", "zom_1", "archived", '"displayed-fleet"'),
    );
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("Delete sends one DELETE with the displayed tag, archives nothing, and leaves for the fleets list", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await openAction(user, /^delete fleet$/i);
    await clickConfirmInDialog(user, /^delete fleet$/i);
    await waitFor(() => expect(routerPush).toHaveBeenCalledWith("/w/ws_1/fleets"));
    expect(deleteFleetActionMock).toHaveBeenCalledExactlyOnceWith("ws_1", "zom_1", '"displayed-fleet"');
    expect(setFleetStatusActionMock).not.toHaveBeenCalled();
  });

  it("a refused Delete stays in its dialog with the action-specific fallback", async () => {
    deleteFleetActionMock.mockResolvedValueOnce({ ok: false, error: "", status: 500 });
    const user = userEvent.setup({ delay: null });
    await renderSwitch("archived");
    await openAction(user, /^delete fleet$/i);
    await clickConfirmInDialog(user, /^delete fleet$/i);
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/Couldn't delete this fleet/i));
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("a rejected Delete transport reports inside the dialog", async () => {
    deleteFleetActionMock.mockRejectedValueOnce(new Error("network down"));
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await openAction(user, /^delete fleet$/i);
    await clickConfirmInDialog(user, /^delete fleet$/i);
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/network down/i));
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("keeps confirmation pending and blocks duplicate terminal requests", async () => {
    let resolveRequest!: (value: { ok: true; data: unknown }) => void;
    setFleetStatusActionMock.mockReturnValueOnce(new Promise((resolve) => {
      resolveRequest = resolve;
    }));
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await openAction(user, /^archive$/i);
    const dialog = await screen.findByRole("alertdialog");
    const confirm = within(dialog).getByRole("button", { name: /^archive$/i });

    fireEvent.click(confirm);
    await waitFor(() => expect(
      (within(dialog).getByRole("button", { name: /working/i }) as HTMLButtonElement).disabled,
    ).toBe(true));
    fireEvent.click(within(dialog).getByRole("button", { name: /working/i }));
    expect(setFleetStatusActionMock).toHaveBeenCalledTimes(1);

    resolveRequest({ ok: true, data: undefined });
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("reports a rejected terminal request and restores the dialog", async () => {
    setFleetStatusActionMock.mockRejectedValueOnce(new Error("network down"));
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await openAction(user, /^archive$/i);
    await clickConfirmInDialog(user, /^archive$/i);

    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/network down/i));
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    expect(
      (within(screen.getByRole("alertdialog")).getByRole("button", { name: /^archive$/i }) as HTMLButtonElement).disabled,
    ).toBe(false);
  });

  it("uses the action fallback when a terminal request rejects without an Error", async () => {
    setFleetStatusActionMock.mockRejectedValueOnce(null);
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await openAction(user, /^archive$/i);
    await clickConfirmInDialog(user, /^archive$/i);

    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(ARCHIVE_FAILURE_PATTERN),
    );
    expect(screen.getByRole("alertdialog")).toBeTruthy();
  });

  it("409 on an already-archived fleet closes the dialog and refreshes", async () => {
    const { ApiError } = await import("../lib/api/errors");
    setFleetStatusMock.mockRejectedValue(new ApiError("fleet is archived", 409, "UZ-AGT-020", "req_x"));
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await openAction(user, /^archive$/i);
    await clickConfirmInDialog(user, /^archive$/i);
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("non-409 error keeps dialog open (status rolled back)", async () => {
    setFleetStatusMock.mockRejectedValue(new Error("network down"));
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await openAction(user, /^archive$/i);
    await clickConfirmInDialog(user, /^archive$/i);
    await waitFor(() => expect(setFleetStatusMock).toHaveBeenCalled());
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
    await openAction(user, /^archive$/i);
    await clickConfirmInDialog(user, /^archive$/i);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Not authenticated/),
    );
    expect(setFleetStatusMock).not.toHaveBeenCalled();
  });

  it("archive action error path renders the action-specific fallback", async () => {
    setFleetStatusActionMock.mockResolvedValueOnce({
      ok: false,
      error: "",
      status: 500,
    });
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await openAction(user, /^archive$/i);
    await clickConfirmInDialog(user, /^archive$/i);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(ARCHIVE_FAILURE_PATTERN),
    );
  });

  // Pins the dialog-dismiss path: clicking Cancel drives onOpenChange(false)
  // which clears pendingAction. Without this, the close-handler line stays
  // uncovered by patch coverage even though every other interaction works.
  it("Cancel dismisses the confirm dialog and clears pendingAction", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitch("active");
    await openAction(user, /^archive$/i);
    const dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: /cancel/i }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(setFleetStatusActionMock).not.toHaveBeenCalled();
  });
});
