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
