import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { CHARGE_TYPE, PROVIDER_MODE } from "@/lib/types";
import { mockAuthOnce as mockAuth } from "./helpers/dashboard-mocks";
import {
  resetDashboardMocks,
  listWorkspaceEventsMock,
  getTenantBillingMock,
  listTenantBillingChargesMock,
} from "./helpers/dashboard-app-mocks";

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
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", async () => (await import("./helpers/dashboard-app-mocks")).fleetActionsMock());
vi.mock("@/lib/api/tenant_billing", async () => (await import("./helpers/dashboard-app-mocks")).tenantBillingMock());
vi.mock("@/lib/api/tenant_provider", async () => (await import("./helpers/dashboard-app-mocks")).tenantProviderMock());
vi.mock("@/app/(dashboard)/settings/billing/components/BillingBalanceCard", async () => (await import("./helpers/dashboard-app-mocks")).billingBalanceCardMock());
vi.mock("@/app/(dashboard)/settings/billing/components/BillingUsageTab", async () => (await import("./helpers/dashboard-app-mocks")).billingUsageTabMock());
vi.mock("@/lib/api/events", async () => (await import("./helpers/dashboard-app-mocks")).eventsMock());
vi.mock("@/app/(dashboard)/actions", async () => (await import("./helpers/dashboard-app-mocks")).dashboardActionsMock());

beforeEach(() => {
  vi.clearAllMocks();
  resetDashboardMocks();
});
afterEach(() => {
  cleanup();
});

describe("placeholder pages", () => {
  it("settings page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null, userId: null });
    const { default: Page } = await import("../app/(dashboard)/settings/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("settings defaults page renders the masked placeholder when authenticated", async () => {
    mockAuth({ token: "tkn" });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/defaults/page");
    const m = renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }));
    expect(m).toContain("Defaults");
  });

  it("settings defaults page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/defaults/page");
    await expect(Page({ params: Promise.resolve({ workspaceId: "ws_1" }) })).rejects.toThrow("redirect:/sign-in");
  });

  it("settings security page renders the masked placeholder when authenticated", async () => {
    mockAuth({ token: "tkn" });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/security/page");
    const m = renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }));
    expect(m).toContain("Security");
  });

  it("settings security page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/security/page");
    await expect(Page({ params: Promise.resolve({ workspaceId: "ws_1" }) })).rejects.toThrow("redirect:/sign-in");
  });

  it("events page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/events/page");
    await expect(Page({ params: Promise.resolve({ workspaceId: "ws_1" }) })).rejects.toThrow("redirect:/sign-in");
  });

  it("events page shell streams the header + skeleton before data", async () => {
    // EventsData is an async child, so renderToStaticMarkup renders the Suspense
    // skeleton in its place — the events section stays absent until it streams
    // in, but the header title paints immediately.
    mockAuth({ token: "token_abc" });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/events/page");
    const m = renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }));
    expect(m).toContain("Events"); // PageTitle in the shell
    expect(m).toContain("data-skeleton"); // Skeleton fallback
    expect(m).not.toContain("Workspace events"); // data not yet resolved
  });

  it("EventsData returns null when the token is missing", async () => {
    mockAuth({ token: null });
    const { EventsData } = await import("../app/(dashboard)/w/[workspaceId]/events/page");
    expect(await EventsData({ workspaceId: "ws_1" })).toBeNull();
  });

  it("events page renders Workspace events section with EventsList", async () => {
    // M118: EventsData reads `workspaceId` from the route param (its prop); the
    // no-workspace `notFound()` is gone (the `[workspaceId]` layout guards it).
    mockAuth({ token: "token_abc" });
    listWorkspaceEventsMock.mockResolvedValue({ items: [], next_cursor: null });
    const { EventsData } = await import("../app/(dashboard)/w/[workspaceId]/events/page");
    const m = renderToStaticMarkup(await EventsData({ workspaceId: "ws_1" }));
    expect(m).toContain("Workspace events");
  });

  it("events page falls back to empty page when listWorkspaceEvents errors", async () => {
    mockAuth({ token: "token_abc" });
    listWorkspaceEventsMock.mockRejectedValue(new Error("boom"));
    const { EventsData } = await import("../app/(dashboard)/w/[workspaceId]/events/page");
    const m = renderToStaticMarkup(await EventsData({ workspaceId: "ws_1" }));
    expect(m).toContain("Workspace events");
  });

  it("models & keys settings page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    await expect(Page({ params: Promise.resolve({ workspaceId: "ws_1" }) })).rejects.toThrow("redirect:/sign-in");
  });

  it("billing settings page renders balance card + usage tab + invoice/payment empty states", async () => {
    mockAuth({ token: "token_billing" });
    getTenantBillingMock.mockResolvedValue({
      balance_nanos: 4_710_000_000,
      updated_at: 1, is_exhausted: false, exhausted_at: null,
    });
    listTenantBillingChargesMock.mockResolvedValue({
      items: [
        {
          id: "tel_1", tenant_id: "t", workspace_id: "w", fleet_id: "z",
          event_id: "evt_1", charge_type: CHARGE_TYPE.receive, posture: PROVIDER_MODE.platform,
          model: "kimi-k2.6", credit_deducted_nanos: 1,
          token_count_input: null, token_count_output: null, wall_ms: null, recorded_at: 1,
        },
      ],
    });
    const { default: Page } = await import("../app/(dashboard)/settings/billing/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Billing");
    expect(m).toContain("data-balance-card=\"1\"");
    expect(m).toContain("data-usage-tab=\"1\"");
    // Radix Tabs only renders the active panel; assert the tab triggers
    // are wired so Invoices / Payment method are reachable on click.
    expect(m).toContain(">Invoices</button>");
    expect(m).toContain(">Payment method</button>");
  });

  it("billing settings page tolerates a /charges 5xx by falling back to empty events", async () => {
    mockAuth({ token: "token_billing" });
    getTenantBillingMock.mockResolvedValue({
      balance_nanos: 0,
      updated_at: 1, is_exhausted: true, exhausted_at: 2,
    });
    listTenantBillingChargesMock.mockRejectedValue(new Error("503"));
    const { default: Page } = await import("../app/(dashboard)/settings/billing/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("data-charge-count=\"0\"");
  });

  it("billing settings page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } = await import("../app/(dashboard)/settings/billing/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("billing settings page shows the not-ready empty state when billing is null", async () => {
    mockAuth({ token: "token_billing" });
    getTenantBillingMock.mockResolvedValue(null);
    listTenantBillingChargesMock.mockResolvedValue({ items: [], next_cursor: null });
    const { default: Page } = await import("../app/(dashboard)/settings/billing/page");
    const m = renderToStaticMarkup(await Page());
    // renderToStaticMarkup escapes the apostrophe in "isn't"; assert on a
    // stable substring of the not-ready empty state instead.
    expect(m).toContain("ready yet");
    expect(m).toContain("Refresh in a moment");
  });

});
