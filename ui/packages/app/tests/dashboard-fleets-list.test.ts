import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetDashboardMocks, listFleetsMock, listFleetsActionMock } from "./helpers/dashboard-app-mocks";

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

describe("FleetsList component", () => {
  const baseFleets = [
    { id: "zom_1", name: "alpha-bot", status: "active", created_at: 1745280000000, updated_at: 1745280000000 },
    { id: "zom_2", name: "beta-bot", status: "paused", created_at: 1745280001000, updated_at: 1745280001000 },
  ];

  async function renderList(props: {
    initialFleets?: typeof baseFleets;
    initialCursor?: string | null;
  } = {}) {
    const { default: FleetsList } = await import(
      "../app/(dashboard)/fleets/components/FleetsList"
    );
    render(
      React.createElement(FleetsList, {
        workspaceId: "ws_1",
        initialFleets: props.initialFleets ?? baseFleets,
        initialCursor: props.initialCursor ?? null,
      } as never),
    );
  }

  it("renders a row per fleet with name + status + id", async () => {
    await renderList();
    expect(screen.getByText("alpha-bot")).toBeTruthy();
    expect(screen.getByText("beta-bot")).toBeTruthy();
    expect(screen.getByText("zom_1")).toBeTruthy();
  });

  it("search filters rows down by name (case-insensitive)", async () => {
    const user = userEvent.setup({ delay: null });
    await renderList();
    await user.type(screen.getByLabelText(/search fleets/i), "ALPHA");
    await waitFor(() => expect(screen.queryByText("beta-bot")).toBeNull());
    expect(screen.getByText("alpha-bot")).toBeTruthy();
  });

  it("search shows empty-match message when nothing matches", async () => {
    const user = userEvent.setup({ delay: null });
    await renderList();
    await user.type(screen.getByLabelText(/search fleets/i), "zzz-no-match");
    await waitFor(() =>
      expect(screen.getByText(/No fleets match/i)).toBeTruthy(),
    );
  });

  it("loadMore: hidden when no cursor", async () => {
    await renderList({ initialCursor: null });
    expect(screen.queryByRole("button", { name: /load more/i })).toBeNull();
  });

  it("loadMore: visible when cursor is present and fetches next page", async () => {
    listFleetsMock.mockResolvedValue({
      items: [
        { id: "zom_3", name: "gamma-bot", status: "active", created_at: "2026-04-22T00:00:02Z" },
      ],
      total: 1,
      cursor: null,
    });
    const user = userEvent.setup({ delay: null });
    await renderList({ initialCursor: "cursor_1" });
    const btn = screen.getByRole("button", { name: /load more/i });
    await user.click(btn);
    await waitFor(() =>
      expect(listFleetsActionMock).toHaveBeenCalledWith("ws_1", { cursor: "cursor_1" }),
    );
    await waitFor(() => expect(screen.getByText("gamma-bot")).toBeTruthy());
  });

  it("loadMore: surfaces fetch error as an alert", async () => {
    listFleetsMock.mockRejectedValue(new Error("boom"));
    const user = userEvent.setup({ delay: null });
    await renderList({ initialCursor: "cursor_1" });
    await user.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/boom/),
    );
  });

  it("loadMore: unauthenticated action result surfaces Not authenticated", async () => {
    listFleetsActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Not authenticated",
      status: 401,
    });
    const user = userEvent.setup({ delay: null });
    await renderList({ initialCursor: "cursor_1" });
    await user.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Not authenticated/),
    );
  });

  it("loadMore: empty error string falls back to default message (covers `||` short-circuit)", async () => {
    listFleetsActionMock.mockResolvedValueOnce({
      ok: false,
      error: "",
      status: 500,
    });
    const user = userEvent.setup({ delay: null });
    await renderList({ initialCursor: "cursor_1" });
    await user.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't load more fleets/),
    );
  });

  it("wake-pulse fires only on active rows (data-live attribute)", async () => {
    await renderList({
      initialFleets: [
        { id: "zom_1", name: "alpha-bot", status: "active", created_at: 1745280000000, updated_at: 1745280000000 },
        { id: "zom_2", name: "beta-bot", status: "paused", created_at: 1745280001000, updated_at: 1745280001000 },
        { id: "zom_3", name: "gamma-bot", status: "killed", created_at: 1745280002000, updated_at: 1745280002000 },
      ],
    });
    const liveRow = screen.getByRole("link", { name: /alpha-bot/i });
    const parkedRow = screen.getByRole("link", { name: /beta-bot/i });
    const failedRow = screen.getByRole("link", { name: /gamma-bot/i });
    expect(liveRow.getAttribute("data-state")).toBe("live");
    expect(parkedRow.getAttribute("data-state")).toBe("parked");
    expect(failedRow.getAttribute("data-state")).toBe("failed");
    expect(liveRow.querySelector("[data-live]")).toBeTruthy();
    expect(parkedRow.querySelector("[data-live]")).toBeFalsy();
    expect(failedRow.querySelector("[data-live]")).toBeFalsy();
  });

  it("wake-pulse cap: only first 5 live rows in render order pulse; rest static", async () => {
    const sixLive = Array.from({ length: 6 }, (_, i) => ({
      id: `zom_${i + 1}`,
      name: `live-${i + 1}`,
      status: "active",
      created_at: 1745280000000 + i,
      updated_at: 1745280000000 + i,
    }));
    await renderList({ initialFleets: sixLive });
    const rows = screen.getAllByRole("link", { name: /live-/i });
    expect(rows).toHaveLength(6);
    const livePulses = rows.filter((r) => r.querySelector("[data-live]"));
    expect(livePulses).toHaveLength(5);
    // Header consolidation count is shown.
    expect(screen.getByLabelText(/6 live/i)).toBeTruthy();
  });

  it("status dot palette: live, parked, failed via data-state", async () => {
    await renderList({
      initialFleets: [
        { id: "zom_1", name: "alpha", status: "active", created_at: 1745280000000, updated_at: 1745280000000 },
        { id: "zom_2", name: "beta", status: "paused", created_at: 1745280001000, updated_at: 1745280001000 },
        { id: "zom_3", name: "gamma", status: "killed", created_at: 1745280002000, updated_at: 1745280002000 },
        { id: "zom_4", name: "delta", status: "stopped", created_at: 1745280003000, updated_at: 1745280003000 },
      ],
    });
    expect(screen.getByRole("link", { name: /alpha/ }).getAttribute("data-state")).toBe("live");
    expect(screen.getByRole("link", { name: /beta/ }).getAttribute("data-state")).toBe("parked");
    expect(screen.getByRole("link", { name: /gamma/ }).getAttribute("data-state")).toBe("failed");
    // `stopped` is parked, not failed — only `killed` maps to the failed dot.
    expect(screen.getByRole("link", { name: /delta/ }).getAttribute("data-state")).toBe("parked");
  });

  // An installing fleet always surfaces its installing state until it resolves
  // — a distinct `installing` data-state + a live indicator + the status label,
  // so create-in-flight progress is never hidden in the list.
  it("test_installing_fleet_always_visible — installing row shows a live installing indicator", async () => {
    await renderList({
      initialFleets: [
        { id: "zom_i", name: "fresh-bot", status: "installing", created_at: 1745280000000, updated_at: 1745280000000 },
        { id: "zom_a", name: "live-bot", status: "active", created_at: 1745280001000, updated_at: 1745280001000 },
      ],
    });
    const installingRow = screen.getByRole("link", { name: /fresh-bot/i });
    expect(installingRow.getAttribute("data-state")).toBe("installing");
    // A live indicator marks it as in-flight (not a dead parked dot).
    expect(installingRow.querySelector("[data-live]")).toBeTruthy();
    // The status label reads "installing" so the state is legible.
    expect(installingRow.textContent).toMatch(/installing/i);
  });
});
