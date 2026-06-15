import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { NANOS_PER_USD } from "@/lib/types";
import { mockAuthOnce as mockAuth, resolveActiveWorkspace as resolveActiveWorkspaceMock } from "./helpers/dashboard-mocks";
import { resetDashboardMocks, listAgentsMock, getTenantBillingMock } from "./helpers/dashboard-app-mocks";

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
vi.mock("@/lib/api/agents", async () => (await import("./helpers/dashboard-app-mocks")).agentsApiMock());
vi.mock("@/app/(dashboard)/agents/actions", async () => (await import("./helpers/dashboard-app-mocks")).agentActionsMock());
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

describe("dashboard overview page", () => {
  it("redirects to /sign-in when no server token", async () => {
    mockAuth({ token: null });
    const { default: Page } = await import("../app/(dashboard)/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("renders page header with Suspense fallbacks when authenticated", async () => {
    const { default: Page } = await import("../app/(dashboard)/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Dashboard");
    expect(m).toContain("data-skeleton");
  });

  it("StatusTiles returns null when no token or no workspace", async () => {
    const mod = await import("../app/(dashboard)/page");
    const Page = mod.default;
    mockAuth({ token: null });
    await expect(Page()).rejects.toThrow("redirect:/sign-in");

    mockAuth({ token: "token_abc" });
    resolveActiveWorkspaceMock.mockResolvedValue(null);
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Dashboard");
  });

  it("StatusTiles renders Live/Paused/Stopped tiles + balance from the agent list", async () => {
    const { StatusTiles } = await import("../app/(dashboard)/page");
    // beforeEach seeds 1 active / 1 paused / 1 stopped; an exhausted balance
    // exercises the `is_exhausted ? "danger"` truthy arm + `active > 0` sublabel.
    getTenantBillingMock.mockResolvedValue({
      balance_nanos: 5 * NANOS_PER_USD,
      is_exhausted: true,
      exhausted_at: 1,
    });
    const m = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles()));
    expect(m).toContain("Live");
    expect(m).toContain("Paused");
    expect(m).toContain("Stopped");
    expect(m).toContain("$5.00"); // billing present → formatted-balance branch

    // No active agents → the sublabel ternary takes its undefined arm while
    // the grid still renders.
    listAgentsMock.mockResolvedValue({
      items: [{ id: "z", name: "n", status: "stopped", created_at: "2026-04-22T00:00:00Z" }],
      total: 1,
      cursor: null,
    });
    getTenantBillingMock.mockResolvedValue(null); // billing null + agents present → Balance "—"
    const m2 = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles()));
    expect(m2).toContain("Stopped");
    expect(m2).toContain("—"); // billing ? ... : "—" false arm
  });

  it("StatusTiles shows the first-install free-credit card when there are no agents", async () => {
    listAgentsMock.mockResolvedValue({ items: [], total: 0, cursor: null });
    getTenantBillingMock.mockResolvedValue({
      balance_nanos: 5 * NANOS_PER_USD,
      is_exhausted: false,
      exhausted_at: null,
    });
    const { StatusTiles } = await import("../app/(dashboard)/page");
    const m = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles()));
    expect(m).toContain("First wake");
    expect(m).toContain("free credit"); // credits > 0 copy branch
  });

  it("StatusTiles first-install copy degrades to the terminal prompt when balance is unknown", async () => {
    listAgentsMock.mockResolvedValue({ items: [], total: 0, cursor: null });
    getTenantBillingMock.mockResolvedValue(null); // balance null → credits-null branch
    const { StatusTiles } = await import("../app/(dashboard)/page");
    const m = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles()));
    expect(m).toContain("First wake");
    expect(m).toContain("Install an agent from your terminal");
  });

  it("StatusTiles returns null without a token or an active workspace", async () => {
    const { StatusTiles } = await import("../app/(dashboard)/page");
    mockAuth({ token: null });
    expect(await StatusTiles()).toBeNull();
    mockAuth({ token: "token_abc" });
    resolveActiveWorkspaceMock.mockResolvedValue(null);
    expect(await StatusTiles()).toBeNull();
  });
});
