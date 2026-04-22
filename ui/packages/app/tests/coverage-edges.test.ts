import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";

// ── auth/server.ts — real module, mock @clerk/nextjs/server ──────────────

const authMock = vi.fn();

vi.mock("@clerk/nextjs/server", () => ({
  auth: authMock,
}));

describe("lib/auth/server", () => {
  beforeEach(() => {
    vi.resetModules();
    authMock.mockReset();
  });

  it("getServerToken returns the minted token string", async () => {
    authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("tkn_abc") });
    const mod = await import("../lib/auth/server");
    expect(await mod.getServerToken()).toBe("tkn_abc");
  });

  it("getServerAuth returns token + userId", async () => {
    authMock.mockResolvedValue({
      getToken: vi.fn().mockResolvedValue("tkn_xyz"),
      userId: "usr_42",
    });
    const mod = await import("../lib/auth/server");
    const out = await mod.getServerAuth();
    expect(out).toEqual({ token: "tkn_xyz", userId: "usr_42" });
  });

  it("getServerAuth normalizes missing userId to null", async () => {
    authMock.mockResolvedValue({
      getToken: vi.fn().mockResolvedValue(null),
      userId: undefined,
    });
    const mod = await import("../lib/auth/server");
    expect(await mod.getServerAuth()).toEqual({ token: null, userId: null });
  });

  it("getServerSessionMetadata returns the metadata record when present", async () => {
    authMock.mockResolvedValue({
      sessionClaims: { metadata: { tenant_id: "t1", workspace_id: "w1" } },
    });
    const mod = await import("../lib/auth/server");
    expect(await mod.getServerSessionMetadata()).toEqual({
      tenant_id: "t1",
      workspace_id: "w1",
    });
  });

  it("getServerSessionMetadata returns null when sessionClaims missing", async () => {
    authMock.mockResolvedValue({ sessionClaims: null });
    const mod = await import("../lib/auth/server");
    expect(await mod.getServerSessionMetadata()).toBeNull();
  });
});

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

    vi.doMock("@/lib/auth/server", () => ({
      getServerToken: vi.fn().mockResolvedValue(token),
      getServerAuth: vi.fn().mockResolvedValue({ token, userId: "u_1" }),
      getServerSessionMetadata: vi.fn().mockResolvedValue(null),
    }));
    vi.doMock("@/lib/workspace", () => ({
      resolveActiveWorkspace: vi.fn().mockResolvedValue(workspace),
      listTenantWorkspacesCached: vi.fn().mockResolvedValue({ items: [], total: 0 }),
    }));
    vi.doMock("@/lib/api/zombies", () => ({
      listZombies: overrides.listZombies ?? vi.fn().mockResolvedValue({ items: [], cursor: null }),
      getZombie: vi.fn(),
      stopZombie: vi.fn(),
    }));
    vi.doMock("@/lib/api/tenant_billing", () => ({
      getTenantBilling: overrides.billing ?? vi.fn().mockResolvedValue({
        balance_cents: 1000, is_exhausted: false, exhausted_at: null,
      }),
    }));
    vi.doMock("@/lib/api/activity", () => ({
      listWorkspaceActivity: overrides.activity ?? vi.fn().mockResolvedValue({ events: [], next_cursor: null }),
      listZombieActivity: vi.fn(),
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

  it("RecentActivity returns null on activity fetch failure (catch arrow)", async () => {
    await withMocks({ activity: () => Promise.reject(new Error("activity-down")) });
    const { RecentActivity } = await import("../app/(dashboard)/page");
    expect(await RecentActivity()).toBeNull();
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
  });

  it("falls back to empty list + null active when there is no token", async () => {
    vi.doMock("@/lib/auth/server", () => ({
      getServerToken: vi.fn().mockResolvedValue(null),
      getServerAuth: vi.fn(),
      getServerSessionMetadata: vi.fn(),
    }));
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
    vi.doMock("@/lib/auth/server", () => ({
      getServerToken: vi.fn().mockResolvedValue("tkn"),
      getServerAuth: vi.fn(),
      getServerSessionMetadata: vi.fn(),
    }));
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
