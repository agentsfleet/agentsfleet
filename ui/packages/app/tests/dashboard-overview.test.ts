import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { NANOS_PER_USD } from "@/lib/types";
import { mockAuthOnce as mockAuth, listTenantWorkspacesCached } from "./helpers/dashboard-mocks";
import { resetDashboardMocks, listFleetsMock, getTenantBillingMock, listWorkspaceFleetLibraryMock } from "./helpers/dashboard-app-mocks";

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
vi.mock("@/lib/api/secrets", async () => (await import("./helpers/dashboard-app-mocks")).secretsApiMock());
vi.mock("@/lib/api/fleet-library", async () => (await import("./helpers/dashboard-app-mocks")).fleetLibraryMock());
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

describe("dashboard overview page", () => {
  it("redirects to /sign-in when no server token", async () => {
    mockAuth({ token: null });
    const { default: Page } = await import("../app/(dashboard)/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("renders page header with Suspense fallbacks when authenticated", async () => {
    // M118: the dashboard home reads its workspace from the `/w/[workspaceId]`
    // route param; the shell (header + Suspense skeleton) paints before the
    // streamed StatusTiles resolve.
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/page");
    const m = renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }));
    expect(m).toContain("Dashboard");
    expect(m).toContain("data-skeleton");
  });

  it("the workspace home redirects to /sign-in without a token", async () => {
    mockAuth({ token: null });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }),
    ).rejects.toThrow("redirect:/sign-in");
  });

  it("root page redirects unauthenticated to sign-in, else to the first owned workspace", async () => {
    // The bare `(dashboard)` root is the one place a default workspace is chosen:
    // no token → redirect to sign-in; authenticated → resolve the first owned
    // workspace and redirect once to its explicit URL. Deeper pages then read
    // the workspace from their route param.
    const { default: Page } = await import("../app/(dashboard)/page");
    mockAuth({ token: null });
    await expect(Page()).rejects.toThrow("redirect:/sign-in");

    mockAuth({ token: "token_abc" });
    listTenantWorkspacesCached.mockResolvedValue({
      items: [{ id: "ws_1", name: "Alpha", created_at: 1 }],
      total: 1,
    });
    await expect(Page()).rejects.toThrow("redirect:/w/ws_1");
  });

  it("StatusTiles renders Live/Paused/Stopped tiles + balance from the fleet list", async () => {
    const { StatusTiles } = await import("../app/(dashboard)/w/[workspaceId]/page");
    // beforeEach seeds 1 active / 1 paused / 1 stopped; an exhausted balance
    // exercises the `is_exhausted ? "danger"` truthy arm + `active > 0` sublabel.
    getTenantBillingMock.mockResolvedValue({
      balance_nanos: 5 * NANOS_PER_USD,
      is_exhausted: true,
      exhausted_at: 1,
    });
    const m = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles({ workspaceId: "ws_1" })));
    expect(m).toContain("Live");
    expect(m).toContain("Paused");
    expect(m).toContain("Stopped");
    expect(m).toContain("$5.00"); // billing present → formatted-balance branch

    // No active fleets → the sublabel ternary takes its undefined arm while
    // the grid still renders.
    listFleetsMock.mockResolvedValue({
      items: [{ id: "z", name: "n", status: "stopped", created_at: "2026-04-22T00:00:00Z" }],
      total: 1,
      cursor: null,
    });
    getTenantBillingMock.mockResolvedValue(null); // billing null + fleets present → Balance "—"
    const m2 = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles({ workspaceId: "ws_1" })));
    expect(m2).toContain("Stopped");
    expect(m2).toContain("—"); // billing ? ... : "—" false arm
  });

  it("StatusTiles shows the first-install free-credit card when there are no fleets", async () => {
    listFleetsMock.mockResolvedValue({ items: [], total: 0, cursor: null });
    getTenantBillingMock.mockResolvedValue({
      balance_nanos: 5 * NANOS_PER_USD,
      is_exhausted: false,
      exhausted_at: null,
    });
    const { StatusTiles } = await import("../app/(dashboard)/w/[workspaceId]/page");
    const m = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles({ workspaceId: "ws_1" })));
    expect(m).toContain("Start your fleet");
    expect(m).toContain("free credit"); // credits > 0 copy branch
  });

  it("StatusTiles first-install copy stays short when balance is unknown", async () => {
    listFleetsMock.mockResolvedValue({ items: [], total: 0, cursor: null });
    getTenantBillingMock.mockResolvedValue(null); // balance null → credits-null branch
    const { StatusTiles } = await import("../app/(dashboard)/w/[workspaceId]/page");
    const m = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles({ workspaceId: "ws_1" })));
    expect(m).toContain("Start your fleet");
    expect(m).not.toContain("free credit"); // balance unknown → no credit pill
  });

  it("StatusTiles first-install card surfaces template cards deep-linking to the install flow", async () => {
    listFleetsMock.mockResolvedValue({ items: [], total: 0, cursor: null });
    getTenantBillingMock.mockResolvedValue(null);
    listWorkspaceFleetLibraryMock.mockResolvedValue({
      items: [
        {
          id: "github-pr-reviewer",
          name: "GitHub PR reviewer",
          description: "Reviews pull requests.",
          visibility: "platform",
          source_ref: "platform/github-pr-reviewer",
          requirements: {
            credentials: ["github"],
            tools: [],
            network_hosts: [],
            trigger_present: true,
          },
          required_credentials_reasons: { github: "review your pull requests" },
          support_files: [],
        },
      ],
    });
    const { StatusTiles } = await import("../app/(dashboard)/w/[workspaceId]/page");
    const m = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles({ workspaceId: "ws_1" })));
    expect(m).toContain("GitHub PR reviewer");
    expect(m).toContain("needs: github");
    // M118: the install deep-link is workspace-scoped (`/w/<id>/fleets/new`).
    expect(m).toContain('href="/w/ws_1/fleets/new?library=github-pr-reviewer"');
  });

  it("StatusTiles first-install card swallows a failed template fetch", async () => {
    listFleetsMock.mockResolvedValue({ items: [], total: 0, cursor: null });
    getTenantBillingMock.mockResolvedValue(null);
    listWorkspaceFleetLibraryMock.mockRejectedValue(new Error("catalog down"));
    const { StatusTiles } = await import("../app/(dashboard)/w/[workspaceId]/page");
    const m = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles({ workspaceId: "ws_1" })));
    expect(m).toContain("Start your fleet"); // card still renders, gallery omitted
  });

  it("StatusTiles returns null without a token", async () => {
    // M118: StatusTiles reads `workspaceId` from its prop (the route), so the
    // only null-guard left is the missing-token one.
    const { StatusTiles } = await import("../app/(dashboard)/w/[workspaceId]/page");
    mockAuth({ token: null });
    expect(await StatusTiles({ workspaceId: "ws_1" })).toBeNull();
  });

  // The dashboard card renders the gallery inline (per-template deep links); the
  // Fleets empty-state is a centered EmptyState that routes to /fleets/new. Both
  // ultimately land on the same install page, but only the dashboard deep-links.
  it("test_install_experience_shared — dashboard card deep-links, fleets routes to install", async () => {
    const templates = {
      items: [
        {
          id: "github-pr-reviewer",
          name: "GitHub PR reviewer",
          description: "Reviews pull requests.",
          visibility: "platform",
          source_ref: "platform/github-pr-reviewer",
          requirements: {
            credentials: ["github"],
            tools: [],
            network_hosts: [],
            trigger_present: true,
          },
          required_credentials_reasons: { github: "review your pull requests" },
          support_files: [],
        },
      ],
    };
    const noFleets = { items: [], total: 0, cursor: null };
    // M118: both surfaces deep-link into the workspace-scoped install route.
    const SHARED_LINK = 'href="/w/ws_1/fleets/new?library=github-pr-reviewer"';

    // Dashboard first-run (StatusTiles → FirstInstall → InstallEntry, compact).
    listFleetsMock.mockResolvedValue(noFleets);
    getTenantBillingMock.mockResolvedValue(null);
    listWorkspaceFleetLibraryMock.mockResolvedValue(templates);
    const { StatusTiles } = await import("../app/(dashboard)/w/[workspaceId]/page");
    const dash = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles({ workspaceId: "ws_1" })));
    expect(dash).toContain(SHARED_LINK);
    expect(dash).not.toContain("Quick start");

    // Fleets empty-state is a centered EmptyState that routes to
    // /w/<id>/fleets/new (where the gallery lives) — no inline per-template deep
    // link, no quickstart.
    listFleetsMock.mockResolvedValue(noFleets);
    getTenantBillingMock.mockResolvedValue(null);
    const { FleetsData } = await import("../app/(dashboard)/w/[workspaceId]/fleets/page");
    const fleets = renderToStaticMarkup(React.createElement(React.Fragment, null, await FleetsData({ workspaceId: "ws_1" })));
    expect(fleets).toContain("No fleets yet");
    expect(fleets).toContain('href="/w/ws_1/fleets/new"');
    expect(fleets).not.toContain(SHARED_LINK);
    expect(fleets).not.toContain("Quick start");
  });
});
