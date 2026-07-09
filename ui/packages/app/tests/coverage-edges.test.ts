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

// ── Billing settings page — Promise.all catch fallbacks ─────────────────

describe("billing settings page — error fallback", () => {
  beforeEach(() => {
    vi.resetModules();
    authMock.mockReset();
  });
  afterEach(() => cleanup());

  it("renders the not-ready empty state when getTenantBilling rejects", async () => {
    authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("tkn") });
    // Both endpoints reject — exercises the `.catch(() => null)` and the
    // `.catch(() => ({ items: [], next_cursor: null }))` fallbacks; a null
    // billing result renders the explanatory empty state, not Next's error page.
    vi.doMock("@/lib/api/tenant_billing", () => ({
      getTenantBilling: vi.fn().mockRejectedValue(new Error("no billing row")),
      listTenantBillingCharges: vi.fn().mockRejectedValue(new Error("no charges")),
    }));
    const { default: BillingSettingsPage } = await import(
      "../app/(dashboard)/settings/billing/page"
    );
    const markup = renderToStaticMarkup(await BillingSettingsPage());
    expect(markup).toMatch(/ready yet/);
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
    listFleets?: () => Promise<unknown>;
    billing?: () => Promise<unknown>;
    activity?: () => Promise<unknown>;
  } = {}) {
    const token = overrides.token === undefined ? "tkn_1" : overrides.token;

    authMock.mockReset();
    authMock.mockResolvedValue({
      getToken: vi.fn().mockResolvedValue(token),
      userId: "u_1",
      sessionClaims: null,
    });
    // M118: `lib/workspace` is slimmed to `listTenantWorkspacesCached`. StatusTiles
    // no longer resolves a workspace — it reads `workspaceId` straight from the
    // page's route param, so the tests pass it as a prop instead.
    vi.doMock("@/lib/workspace", () => ({
      listTenantWorkspacesCached: vi.fn().mockResolvedValue({ items: [], total: 0 }),
    }));
    // The empty-fleets branch pulls the first-run library gallery; mock it so the
    // catch path is deterministic rather than a real (failing) fetch.
    vi.doMock("@/lib/api/fleet-library", () => ({
      listWorkspaceFleetLibrary: vi.fn().mockResolvedValue({ items: [] }),
      listWorkspaceFleetLibraryCached: vi.fn().mockResolvedValue({ items: [] }),
      onboardWorkspaceFleetLibrary: vi.fn(),
    }));
    vi.doMock("@/lib/api/fleets", () => ({
      listFleets: overrides.listFleets ?? vi.fn().mockResolvedValue({ items: [], cursor: null }),
      getFleet: vi.fn(),
      stopFleet: vi.fn(),
      AGENTSFLEET_STATUS: {
        ACTIVE: "active",
        PAUSED: "paused",
        STOPPED: "stopped",
        KILLED: "killed",
        ERRORED: "errored",
      },
    }));
    {
      const billingFn = overrides.billing ?? vi.fn().mockResolvedValue({
        balance_nanos: NANOS_PER_USD, is_exhausted: false, exhausted_at: null,
      });
      vi.doMock("@/lib/api/tenant_billing", () => ({
        getTenantBilling: billingFn,
        getTenantBillingCached: billingFn,
      }));
    }
    vi.doMock("@/lib/api/events", () => ({
      listWorkspaceEvents: overrides.activity ?? vi.fn().mockResolvedValue({ items: [], next_cursor: null }),
      listFleetEvents: vi.fn().mockResolvedValue({ items: [], next_cursor: null }),
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
    const { StatusTiles } = await import("../app/(dashboard)/w/[workspaceId]/page");
    expect(await StatusTiles({ workspaceId: "ws_1" })).toBeNull();
  });

  it("StatusTiles renders counts and hits the listFleets catch branch", async () => {
    await withMocks({ listFleets: () => Promise.reject(new Error("boom")) });
    const { StatusTiles } = await import("../app/(dashboard)/w/[workspaceId]/page");
    const element = await StatusTiles({ workspaceId: "ws_1" });
    expect(element).not.toBeNull();
  });

  it("StatusTiles tolerates getTenantBilling failure (billing catch arrow)", async () => {
    await withMocks({ billing: () => Promise.reject(new Error("billing-down")) });
    const { StatusTiles } = await import("../app/(dashboard)/w/[workspaceId]/page");
    const element = await StatusTiles({ workspaceId: "ws_1" });
    expect(element).not.toBeNull();
  });

  it("StatusTiles renders the dollar-formatted balance when fleets exist + billing succeeds", async () => {
    await withMocks({
      listFleets: vi.fn().mockResolvedValue({
        items: [
          { id: "zom_1", name: "alpha", status: "active", created_at: "2026-04-22T00:00:00Z" },
        ],
        cursor: null,
      }),
      billing: vi.fn().mockResolvedValue({
        balance_nanos: 4_710_000_000, is_exhausted: false, exhausted_at: null,
      }),
    });
    const { StatusTiles } = await import("../app/(dashboard)/w/[workspaceId]/page");
    const element = await StatusTiles({ workspaceId: "ws_1" });
    expect(element).not.toBeNull();
    const html = renderToStaticMarkup(element as React.ReactElement);
    expect(html).toContain("$4.71");
  });

  it("StatusTiles renders the em-dash balance fallback when fleets exist + billing is null", async () => {
    await withMocks({
      listFleets: vi.fn().mockResolvedValue({
        items: [
          { id: "zom_1", name: "alpha", status: "active", created_at: "2026-04-22T00:00:00Z" },
        ],
        cursor: null,
      }),
      billing: () => Promise.reject(new Error("billing-down")),
    });
    const { StatusTiles } = await import("../app/(dashboard)/w/[workspaceId]/page");
    const element = await StatusTiles({ workspaceId: "ws_1" });
    expect(element).not.toBeNull();
    const html = renderToStaticMarkup(element as React.ReactElement);
    expect(html).toContain("—");
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
