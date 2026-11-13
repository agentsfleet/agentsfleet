import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FLEET } from "./helpers/draft-fixtures";
import { routerPush, routerRefresh, fetchMock, resetCommonMocks, authMock as auth } from "./helpers/dashboard-mocks";

// Shared dashboard mock harness — see tests/helpers/dashboard-mocks.tsx.
vi.stubGlobal("fetch", fetchMock);
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("@clerk/nextjs/server", async () => (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("@clerk/nextjs", async () => (await import("./helpers/dashboard-mocks")).clerkMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@/lib/workspace", async () => (await import("./helpers/dashboard-mocks")).workspaceMock());
vi.mock("lucide-react", async () => (await import("./helpers/dashboard-mocks")).lucideMock());
vi.mock("@agentsfleet/design-system", async (orig) => {
  const h = await import("./helpers/dashboard-mocks");
  return { ...h.designSystemCore(await orig<Record<string, unknown>>()), ...h.designSystemTabs() };
});

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks({ pathname: "/w/ws_1/fleets" });
});
afterEach(() => {
  cleanup();
  fetchMock.mockReset();
});

// ── ExhaustionBadge + ExhaustionBanner ─────────────────────────────────────

describe("exhaustion components", () => {
  it("ExhaustionBadge renders with a timestamped tooltip when exhaustedAt set", async () => {
    const { default: ExhaustionBadge } = await import(
      "../components/domain/ExhaustionBadge"
    );
    render(React.createElement(ExhaustionBadge, { exhaustedAt: "2024-04-21T11:46:40Z" }));
    const el = screen.getByRole("status", { name: /balance exhausted/i });
    expect(el).toBeTruthy();
    expect(el.getAttribute("title")).toMatch(/^Exhausted since /);
  });

  it("ExhaustionBadge renders with generic title when exhaustedAt is null", async () => {
    const { default: ExhaustionBadge } = await import(
      "../components/domain/ExhaustionBadge"
    );
    render(React.createElement(ExhaustionBadge, { exhaustedAt: null }));
    expect(screen.getByRole("status").getAttribute("title")).toBe("Balance exhausted");
  });

  it("ExhaustionBanner renders nothing when billing is null", async () => {
    const { default: ExhaustionBanner } = await import(
      "../components/domain/ExhaustionBanner"
    );
    const { container } = render(
      React.createElement(ExhaustionBanner, { billing: null }),
    );
    expect(container.innerHTML).toBe("");
  });

  it("ExhaustionBanner renders nothing when not exhausted", async () => {
    const { default: ExhaustionBanner } = await import(
      "../components/domain/ExhaustionBanner"
    );
    const { container } = render(
      React.createElement(ExhaustionBanner, {
        billing: {
          balance_nanos: "500000000",
          updated_at: "1970-01-01T00:00:00Z",
          is_exhausted: false,
          exhausted_at: null,
        },
      }),
    );
    expect(container.innerHTML).toBe("");
  });

  it("ExhaustionBanner renders destructive alert with timestamp when exhausted", async () => {
    const { default: ExhaustionBanner } = await import(
      "../components/domain/ExhaustionBanner"
    );
    render(
      React.createElement(ExhaustionBanner, {
        billing: {
          balance_nanos: "0",
          updated_at: "2024-04-21T11:46:40Z",
          is_exhausted: true,
          exhausted_at: "2024-04-21T11:53:20Z",
        },
      }),
    );
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("credit balance is exhausted");
    expect(alert.textContent).toContain("BALANCE_EXHAUSTED_POLICY");
    expect(alert.querySelector("a[href^='mailto:']")).toBeTruthy();
    expect(alert.textContent).toMatch(/Exhausted since /);
  });

  it("ExhaustionBanner omits timestamp text when exhausted_at is null", async () => {
    const { default: ExhaustionBanner } = await import(
      "../components/domain/ExhaustionBanner"
    );
    render(
      React.createElement(ExhaustionBanner, {
        billing: {
          balance_nanos: "0",
          updated_at: "1970-01-01T00:00:00Z",
          is_exhausted: true,
          exhausted_at: null,
        },
      }),
    );
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("credit balance is exhausted");
    expect(alert.textContent).not.toMatch(/Exhausted since /);
  });
});

// ── Fleet actions — delete flow ───────────────────────────────────────────

describe("Fleet actions delete", () => {
  async function openDelete(status = "active") {
    const { default: FleetActions } = await import(
      "../app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetActions"
    );
    const { DELETE_MEMORY_TRAP_NOTICE } = await import(
      "../app/(dashboard)/w/[workspaceId]/fleets/[id]/components/console-copy"
    );
    const user = userEvent.setup({ delay: null });
    render(
      React.createElement(FleetActions, {
        workspaceId: "ws_1",
        fleet: { id: "zom_1", name: "Pull request review", status, etag: FLEET.etag },
      }),
    );
    await user.click(screen.getByRole("button", { name: "Fleet actions" }));
    await user.click(screen.getByRole("menuitem", { name: /^delete fleet$/i }));
    const dialog = screen.getByRole("alertdialog");
    // The confirm states memory dies with the fleet, and that editing keeps it (G8).
    expect(dialog.textContent).toContain(DELETE_MEMORY_TRAP_NOTICE);
    return { user, dialog };
  }

  it.each(["active", "installing", "archived"])("deletes a %s fleet with one DELETE and leaves for /w/ws_1/fleets", async (status) => {
    fetchMock.mockResolvedValue({ ok: true, status: 204 });
    const { user, dialog } = await openDelete(status);
    await user.click(within(dialog).getByRole("button", { name: /^delete fleet$/i }));

    await waitFor(() => expect(routerPush).toHaveBeenCalledWith("/w/ws_1/fleets"));
    // Delete ends live work itself: nothing archives first.
    expect(fetchMock).toHaveBeenCalledOnce();
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_1"),
      expect.objectContaining({ method: "DELETE", headers: expect.objectContaining({"If-Match": FLEET.etag}) }),
    );
    // router.refresh() is intentionally NOT called — refresh-after-push races
    // the current-route refetch against push's URL commit. The fleets list is
    // `force-dynamic` so refresh isn't needed.
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("surfaces an unauthenticated action as the session-expired copy", async () => {
    // Server action mints the Bearer via `auth()` on @clerk/nextjs/server.
    // Null getToken there is what the harness sees in the unauthenticated path.
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { user, dialog } = await openDelete();
    await user.click(within(dialog).getByRole("button", { name: /^delete fleet$/i }));
    // 401 from withToken maps to UZ-AUTH-401, which presentError renders as
    // the curated "Your session expired" copy (WS-G).
    await waitFor(() => expect(screen.getByText(/Your session expired/i)).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("renders a server error inside the dialog without navigating", async () => {
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 503,
      statusText: "Service Unavailable",
      json: async () => ({ detail: "storage unavailable" }),
    });
    const { user, dialog } = await openDelete();
    await user.click(within(dialog).getByRole("button", { name: /^delete fleet$/i }));
    await waitFor(() => expect(within(screen.getByRole("alertdialog")).getByRole("alert")).toBeTruthy());
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("Cancel dismisses the confirm dialog without a request", async () => {
    const { user, dialog } = await openDelete();
    await user.click(within(dialog).getByRole("button", { name: /cancel/i }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
