import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { routerPush, routerRefresh, fetchMock, resetCommonMocks, authMock as auth } from "./helpers/dashboard-mocks";

// Shared dashboard mock harness — see tests/helpers/dashboard-mocks.tsx.
vi.stubGlobal("fetch", fetchMock);
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("@clerk/nextjs/server", async () => (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("@clerk/nextjs", async () => (await import("./helpers/dashboard-mocks")).clerkMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@/lib/workspace", async () => (await import("./helpers/dashboard-mocks")).workspaceMock());
vi.mock("lucide-react", async () => (await import("./helpers/dashboard-mocks")).lucideMock());
vi.mock("@usezombie/design-system", async (orig) => {
  const h = await import("./helpers/dashboard-mocks");
  return { ...h.designSystemCore(await orig<Record<string, unknown>>()), ...h.designSystemTabs() };
});

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks({ pathname: "/zombies" });
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
    render(React.createElement(ExhaustionBadge, { exhaustedAt: 1713700000000 }));
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
          balance_nanos: 500_000_000,
          updated_at: 0,
          is_exhausted: false,
          exhausted_at: null,
          free_trial: { active: false, ends_at_ms: 1_785_542_400_000 },
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
          balance_nanos: 0,
          updated_at: 1713700000000,
          is_exhausted: true,
          exhausted_at: 1713700400000,
          free_trial: { active: false, ends_at_ms: 1_785_542_400_000 },
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
          balance_nanos: 0,
          updated_at: 0,
          is_exhausted: true,
          exhausted_at: null,
          free_trial: { active: false, ends_at_ms: 1_785_542_400_000 },
        },
      }),
    );
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("credit balance is exhausted");
    expect(alert.textContent).not.toMatch(/Exhausted since /);
  });
});

// ── ZombieConfig — delete flow ─────────────────────────────────────────────

describe("ZombieConfig interactions", () => {
  it("delete flow: confirm dialog → DELETE call → push to /zombies", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204 });
    const { default: ZombieConfig } = await import(
      "../app/(dashboard)/zombies/[id]/components/ZombieConfig"
    );
    const user = userEvent.setup({ delay: null });
    render(
      React.createElement(ZombieConfig, {
        workspaceId: "ws_1",
        zombieId: "zom_1",
        zombieName: "platform-ops",
      }),
    );

    await user.click(screen.getByRole("button", { name: /delete zombie/i }));
    // Confirm dialog is now visible.
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /yes, delete/i }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        expect.stringContaining("/v1/workspaces/ws_1/zombies/zom_1"),
        expect.objectContaining({ method: "DELETE" }),
      ),
    );
    expect(routerPush).toHaveBeenCalledWith("/zombies");
    // router.refresh() is intentionally NOT called — refresh-after-push races
    // the current-route refetch against push's URL commit (same race the
    // InstallZombieForm hit). /zombies is `force-dynamic` so refresh isn't
    // needed.
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("delete flow: missing token blocks the call and surfaces an error", async () => {
    // Server action mints the Bearer via `auth()` on @clerk/nextjs/server.
    // Null getToken there is what the harness sees in the unauthenticated path.
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: ZombieConfig } = await import(
      "../app/(dashboard)/zombies/[id]/components/ZombieConfig"
    );
    const user = userEvent.setup({ delay: null });
    render(
      React.createElement(ZombieConfig, {
        workspaceId: "ws_1",
        zombieId: "zom_1",
        zombieName: "platform-ops",
      }),
    );
    await user.click(screen.getByRole("button", { name: /delete zombie/i }));
    await user.click(screen.getByRole("button", { name: /yes, delete/i }));
    // 401 from withToken maps to UZ-AUTH-401, which presentError renders as
    // the curated "Your session expired" copy — no raw "Not authenticated"
    // string surfaces to the operator (WS-G).
    await waitFor(() =>
      expect(screen.getByText(/Your session expired/i)).toBeTruthy(),
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("delete flow: server error is rendered inline without navigating", async () => {
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 409,
      statusText: "Conflict",
      json: async () => ({ detail: "run in progress", error_code: "UZ-ZOM-004" }),
    });
    const { default: ZombieConfig } = await import(
      "../app/(dashboard)/zombies/[id]/components/ZombieConfig"
    );
    const user = userEvent.setup({ delay: null });
    render(
      React.createElement(ZombieConfig, {
        workspaceId: "ws_1",
        zombieId: "zom_1",
        zombieName: "platform-ops",
      }),
    );
    await user.click(screen.getByRole("button", { name: /delete zombie/i }));
    await user.click(screen.getByRole("button", { name: /yes, delete/i }));
    await waitFor(() =>
      expect(screen.getByText(/run in progress/i)).toBeTruthy(),
    );
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("delete flow: Cancel dismisses the confirm dialog", async () => {
    const { default: ZombieConfig } = await import(
      "../app/(dashboard)/zombies/[id]/components/ZombieConfig"
    );
    const user = userEvent.setup({ delay: null });
    render(
      React.createElement(ZombieConfig, {
        workspaceId: "ws_1",
        zombieId: "zom_1",
        zombieName: "platform-ops",
      }),
    );
    await user.click(screen.getByRole("button", { name: /delete zombie/i }));
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
