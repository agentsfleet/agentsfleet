import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { NANOS_PER_USD } from "@/lib/types";

// Post-Stage-1: dashboard pages call `auth().getToken()` directly from
// `@clerk/nextjs/server` — no `lib/auth/server.ts` indirection. Tests mock
// `auth` directly and feed `getToken` / `sessionClaims` / `userId` via
// per-case overrides.

const authMock = vi.fn();

vi.mock("@clerk/nextjs/server", () => ({
  auth: authMock,
}));

// ── lib/utils.ts — formatDate (currently uncovered) ──────────────────────

describe("lib/utils formatDate", () => {
  it("formats a Date instance into the en-US short form", async () => {
    const { formatDate } = await import("../lib/utils");
    const out = formatDate(new Date("2026-04-22T18:30:00Z"));
    expect(typeof out).toBe("string");
    expect(out.length).toBeGreaterThan(0);
    // Output contains the year.
    expect(out).toMatch(/2026/);
  });

  it("accepts an ISO string input", async () => {
    const { formatDate } = await import("../lib/utils");
    expect(formatDate("2026-04-22T00:00:00Z")).toMatch(/2026/);
  });
});

// ── Inner async components of dashboard page.tsx + layout edge branches ──

describe("dashboard page inner async components", () => {
  beforeEach(() => {
    vi.resetModules();
    authMock.mockReset();
    authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("tkn_1") });
  });

  async function withMocks(overrides: {
    token?: string | null;
    workspace?: { id: string; name: string } | null;
    listZombies?: () => Promise<unknown>;
    billing?: () => Promise<unknown>;
    activity?: () => Promise<unknown>;
  } = {}) {
    const token = overrides.token === undefined ? "tkn_1" : overrides.token;
    const workspace = overrides.workspace === undefined
      ? { id: "ws_1", name: "Alpha" }
      : overrides.workspace;

    authMock.mockReset();
    authMock.mockResolvedValue({
      getToken: vi.fn().mockResolvedValue(token),
      userId: "u_1",
      sessionClaims: null,
    });
    vi.doMock("@/lib/workspace", () => ({
      resolveActiveWorkspace: vi.fn().mockResolvedValue(workspace),
      listTenantWorkspacesCached: vi.fn().mockResolvedValue({ items: [], total: 0 }),
    }));
    vi.doMock("@/lib/api/zombies", () => ({
      listZombies: overrides.listZombies ?? vi.fn().mockResolvedValue({ items: [], cursor: null }),
      getZombie: vi.fn(),
      stopZombie: vi.fn(),
      ZOMBIE_STATUS: {
        ACTIVE: "active",
        PAUSED: "paused",
        STOPPED: "stopped",
        KILLED: "killed",
        ERRORED: "errored",
      },
    }));
    vi.doMock("@/lib/api/tenant_billing", () => ({
      getTenantBilling: overrides.billing ?? vi.fn().mockResolvedValue({
        balance_nanos: NANOS_PER_USD, is_exhausted: false, exhausted_at: null,
      }),
    }));
    vi.doMock("@/lib/api/events", () => ({
      listWorkspaceEvents: overrides.activity ?? vi.fn().mockResolvedValue({ items: [], next_cursor: null }),
      listZombieEvents: vi.fn().mockResolvedValue({ items: [], next_cursor: null }),
    }));
    vi.doMock("next/navigation", () => ({
      notFound: vi.fn(() => { throw new Error("notFound"); }),
      redirect: vi.fn(),
      usePathname: vi.fn(() => "/"),
      useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
    }));
  }

  it("StatusTiles returns null when token missing", async () => {
    await withMocks({ token: null });
    const { StatusTiles } = await import("../app/(dashboard)/page");
    expect(await StatusTiles()).toBeNull();
  });

  it("StatusTiles returns null when workspace missing", async () => {
    await withMocks({ workspace: null });
    const { StatusTiles } = await import("../app/(dashboard)/page");
    expect(await StatusTiles()).toBeNull();
  });

  it("StatusTiles renders counts and hits the listZombies catch branch", async () => {
    await withMocks({ listZombies: () => Promise.reject(new Error("boom")) });
    const { StatusTiles } = await import("../app/(dashboard)/page");
    const element = await StatusTiles();
    expect(element).not.toBeNull();
  });

  it("StatusTiles tolerates getTenantBilling failure (billing catch arrow)", async () => {
    await withMocks({ billing: () => Promise.reject(new Error("billing-down")) });
    const { StatusTiles } = await import("../app/(dashboard)/page");
    const element = await StatusTiles();
    expect(element).not.toBeNull();
  });

  it("StatusTiles renders the dollar-formatted balance when zombies exist + billing succeeds", async () => {
    await withMocks({
      listZombies: vi.fn().mockResolvedValue({
        items: [
          { id: "zom_1", name: "alpha", status: "active", created_at: "2026-04-22T00:00:00Z" },
        ],
        cursor: null,
      }),
      billing: vi.fn().mockResolvedValue({
        balance_nanos: 4_710_000_000, is_exhausted: false, exhausted_at: null,
      }),
    });
    const { StatusTiles } = await import("../app/(dashboard)/page");
    const element = await StatusTiles();
    expect(element).not.toBeNull();
    const html = renderToStaticMarkup(element as React.ReactElement);
    expect(html).toContain("$4.71");
  });

  it("StatusTiles renders the em-dash balance fallback when zombies exist + billing is null", async () => {
    await withMocks({
      listZombies: vi.fn().mockResolvedValue({
        items: [
          { id: "zom_1", name: "alpha", status: "active", created_at: "2026-04-22T00:00:00Z" },
        ],
        cursor: null,
      }),
      billing: () => Promise.reject(new Error("billing-down")),
    });
    const { StatusTiles } = await import("../app/(dashboard)/page");
    const element = await StatusTiles();
    expect(element).not.toBeNull();
    const html = renderToStaticMarkup(element as React.ReactElement);
    expect(html).toContain("—");
  });

  it("RecentActivity returns null when token missing", async () => {
    await withMocks({ token: null });
    const { RecentActivity } = await import("../app/(dashboard)/page");
    expect(await RecentActivity()).toBeNull();
  });

  it("RecentActivity returns null when workspace missing", async () => {
    await withMocks({ workspace: null });
    const { RecentActivity } = await import("../app/(dashboard)/page");
    expect(await RecentActivity()).toBeNull();
  });

  it("RecentActivity falls back to an empty events page on fetch failure", async () => {
    await withMocks({ activity: vi.fn().mockRejectedValue(new Error("events-down")) });
    const { RecentActivity } = await import("../app/(dashboard)/page");
    const element = await RecentActivity();
    // Slice 10 changed the failure mode: instead of returning null and
    // hiding the panel, render the events list with an empty initial
    // page. Operators still see the section header + EmptyState.
    expect(element).not.toBeNull();
  });

  it("RecentActivity renders the feed on success", async () => {
    await withMocks();
    const { RecentActivity } = await import("../app/(dashboard)/page");
    const element = await RecentActivity();
    expect(element).not.toBeNull();
  });
});

// ── DashboardLayout — null token + catch branch ─────────────────────────

describe("DashboardLayout edge branches", () => {
  beforeEach(() => {
    vi.resetModules();
    authMock.mockReset();
  });

  it("falls back to empty list + null active when there is no token", async () => {
    authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    vi.doMock("@/lib/workspace", () => ({
      resolveActiveWorkspace: vi.fn(),
      listTenantWorkspacesCached: vi.fn(),
    }));
    vi.doMock("@/components/layout/Shell", () => ({
      default: ({ workspaces, activeWorkspaceId, children }: {
        workspaces: unknown[]; activeWorkspaceId: string | null; children: React.ReactNode;
      }) => React.createElement("div", {
        "data-ws-count": String((workspaces ?? []).length),
        "data-active": activeWorkspaceId ?? "none",
      }, children),
    }));
    const { default: DashboardLayout } = await import("../app/(dashboard)/layout");
    const markup = renderToStaticMarkup(
      await DashboardLayout({ children: React.createElement("span", null, "x") }),
    );
    expect(markup).toContain('data-ws-count="0"');
    expect(markup).toContain('data-active="none"');
  });

  it("recovers via catch when listTenantWorkspacesCached rejects", async () => {
    authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("tkn") });
    vi.doMock("@/lib/workspace", () => ({
      resolveActiveWorkspace: vi.fn().mockResolvedValue(null),
      listTenantWorkspacesCached: vi.fn().mockRejectedValue(new Error("api-down")),
    }));
    vi.doMock("@/components/layout/Shell", () => ({
      default: ({ workspaces, children }: {
        workspaces: unknown[]; children: React.ReactNode;
      }) => React.createElement("div", {
        "data-ws-count": String((workspaces ?? []).length),
      }, children),
    }));
    const { default: DashboardLayout } = await import("../app/(dashboard)/layout");
    const markup = renderToStaticMarkup(
      await DashboardLayout({ children: React.createElement("span", null, "ok") }),
    );
    expect(markup).toContain('data-ws-count="0"');
    expect(markup).toContain("ok");
  });
});

afterEach(() => {
  cleanup();
});
