import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { NANOS_PER_USD } from "@/lib/types";
import { resolveActiveWorkspace, fetchMock, resetCommonMocks, authMock as auth } from "./helpers/dashboard-mocks";
import { listFleetTemplatesMock, listCredentialsMock } from "./helpers/dashboard-app-mocks";

type BillingSnapshot = {
  balance_nanos: number;
  updated_at: number;
  is_exhausted: boolean;
  exhausted_at: number | null;
};

// Shared dashboard mock harness — see tests/helpers/dashboard-mocks.tsx.
vi.stubGlobal("fetch", fetchMock);
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("@clerk/nextjs/server", async () => (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("@clerk/nextjs", async () => (await import("./helpers/dashboard-mocks")).clerkMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@/lib/workspace", async () => (await import("./helpers/dashboard-mocks")).workspaceMock());
vi.mock("@/components/domain/FleetApprovalsPanel", () => ({
  default: () => React.createElement("div", { "data-stub": "FleetApprovalsPanel" }),
}));
vi.mock("lucide-react", async () => (await import("./helpers/dashboard-mocks")).lucideMock());
vi.mock("@agentsfleet/design-system", async (orig) => {
  const h = await import("./helpers/dashboard-mocks");
  return { ...h.designSystemCore(await orig<Record<string, unknown>>()), ...h.designSystemTabs() };
});
vi.mock("@/lib/api/fleet-bundles", async () => (await import("./helpers/dashboard-app-mocks")).fleetBundlesMock());
vi.mock("@/lib/api/credentials", async () => (await import("./helpers/dashboard-app-mocks")).credentialsApiMock());

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks({ pathname: "/fleets" });
  // The Fleets empty-state lazily fetches the template gallery; default it to an
  // empty catalog so tests that don't care about templates don't crash on the
  // unmocked promise (individual tests override as needed).
  listFleetTemplatesMock.mockResolvedValue({ items: [] });
});
afterEach(() => {
  cleanup();
  fetchMock.mockReset();
});

// ── Fleets route — page, loading, detail, new ─────────────────────────────

describe("fleets routes", () => {
  const happyBilling: BillingSnapshot = {
    balance_nanos: NANOS_PER_USD,
    updated_at: 0,
    is_exhausted: false,
    exhausted_at: null,
  };
  const exhaustedBilling: BillingSnapshot = {
    ...happyBilling,
    is_exhausted: true,
    exhausted_at: 1,
  };

  function mockFetchBilling(billing: BillingSnapshot) {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => billing };
      }
      if (url.includes("/approvals")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      if (url.includes("/events")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({
          items: [
            {
              id: "zom_1",
              name: "platform-ops",
              status: "active",
              created_at: 1713700000000,
              updated_at: 1713700000000,
            },
          ],
          total: 1,
        }),
      };
    });
  }

  it("loading.tsx renders a spinner with status role", async () => {
    const { default: Loading } = await import("../app/(dashboard)/fleets/loading");
    render(React.createElement(Loading));
    const el = screen.getByRole("status");
    expect(el.textContent).toContain("Loading fleets");
    // Branded WakePulse dot (data-live), not the off-system Loader2Icon spin.
    const dot = el.querySelector("[data-live]");
    expect(dot).toBeTruthy();
    expect(dot?.className).toContain("bg-pulse");
  });

  it("fleets list page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/fleets/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("fleets list page renders empty-workspace state", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce(null);
    const { default: Page } = await import("../app/(dashboard)/fleets/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("No workspace yet");
  });

  it("fleets list page renders empty-fleets state with banner suppressed", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({ items: [], total: 0, next_cursor: null }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/fleets/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("Start your fleet");
    // The empty-state composes the shared InstallEntry (its source affordance).
    expect(markup).toContain("Import from GitHub or paste SKILL.md");
    expect(markup).not.toContain("credit balance is exhausted");
  });

  it("fleets list page renders populated list + exhaustion banner", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    mockFetchBilling(exhaustedBilling);
    const { default: Page } = await import("../app/(dashboard)/fleets/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("href=\"/fleets/zom_1\"");
    expect(markup).toContain("platform-ops");
    expect(markup).toContain("credit balance is exhausted");
  });

  it("fleets list empty-state swallows a failed template fetch (gallery omitted)", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    listFleetTemplatesMock.mockRejectedValueOnce(new Error("catalog down"));
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      return { ok: true, status: 200, json: async () => ({ items: [], total: 0, next_cursor: null }) };
    });
    const { default: Page } = await import("../app/(dashboard)/fleets/page");
    const markup = renderToStaticMarkup(await Page());
    // Card still renders via the catch → [] arm; the shared source affordance shows.
    expect(markup).toContain("Start your fleet");
    expect(markup).toContain("Import from GitHub or paste SKILL.md");
  });

  it("fleets list page swallows a failed billing fetch and still renders", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: false, status: 500, statusText: "err", json: async () => ({}) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({ items: [], total: 0, next_cursor: null }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/fleets/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("Start your fleet");
  });

  it("fleets new page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/fleets/new/page");
    await expect(Page({ searchParams: Promise.resolve({}) })).rejects.toThrow("redirect:/sign-in");
  });

  it("fleets new page renders empty-workspace guard", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce(null);
    const { default: Page } = await import("../app/(dashboard)/fleets/new/page");
    const markup = renderToStaticMarkup(await Page({ searchParams: Promise.resolve({}) }));
    expect(markup).toContain("Create a workspace before installing a fleet");
  });

  it("fleets new page renders the gallery-first install flow when a workspace exists", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    listFleetTemplatesMock.mockResolvedValue({
      items: [
        {
          id: "github-pr-reviewer",
          name: "GitHub PR reviewer",
          description: "Reviews pull requests.",
          required_credentials: ["github"],
          required_tools: [],
          network_hosts: [],
        },
      ],
    });
    listCredentialsMock.mockResolvedValue({ credentials: [{ name: "github", created_at: 1 }] });
    const { default: Page } = await import("../app/(dashboard)/fleets/new/page");
    const markup = renderToStaticMarkup(await Page({ searchParams: Promise.resolve({}) }));
    expect(markup).toContain("Install fleet"); // page title
    expect(markup).toContain("Start from a template");
    expect(markup).toContain("GitHub PR reviewer");
    expect(markup).toContain("Import from GitHub"); // GitHub source-strip action
  });

  it("fleets new page swallows failed template + credential fetches", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    listFleetTemplatesMock.mockRejectedValue(new Error("catalog down"));
    listCredentialsMock.mockRejectedValue(new Error("vault down"));
    const { default: Page } = await import("../app/(dashboard)/fleets/new/page");
    const markup = renderToStaticMarkup(await Page({ searchParams: Promise.resolve({}) }));
    expect(markup).toContain("No templates available yet"); // empty gallery → EmptyState
  });

  it("fleets new page accepts a ?template= deep link", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    listFleetTemplatesMock.mockResolvedValue({ items: [] });
    listCredentialsMock.mockResolvedValue({ credentials: [] });
    const { default: Page } = await import("../app/(dashboard)/fleets/new/page");
    const markup = renderToStaticMarkup(
      await Page({ searchParams: Promise.resolve({ template: "github-pr-reviewer" }) }),
    );
    expect(markup).toContain("Start from a template");
  });

  it("fleets detail page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/fleets/[id]/page");
    await expect(Page({ params: Promise.resolve({ id: "zom_1" }) })).rejects.toThrow(
      "redirect:/sign-in",
    );
  });

  it("fleets detail page notFound when no workspace", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce(null);
    const { default: Page } = await import("../app/(dashboard)/fleets/[id]/page");
    await expect(Page({ params: Promise.resolve({ id: "zom_1" }) })).rejects.toThrow(
      "notFound",
    );
  });

  it("fleets detail page notFound when fleet id is not in the list", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    mockFetchBilling(happyBilling);
    const { default: Page } = await import("../app/(dashboard)/fleets/[id]/page");
    await expect(
      Page({ params: Promise.resolve({ id: "missing" }) }),
    ).rejects.toThrow("notFound");
  });

  it("fleets detail page renders panels + exhaustion badge when tenant is exhausted", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    mockFetchBilling(exhaustedBilling);
    const { default: Page } = await import("../app/(dashboard)/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toContain("platform-ops");
    expect(markup).toContain("Trigger");
    expect(markup).toContain("Configuration");
    expect(markup).toContain("Balance exhausted");
  });

  it("fleets detail page renders without badge when not exhausted", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    mockFetchBilling(happyBilling);
    const { default: Page } = await import("../app/(dashboard)/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).not.toContain("Balance exhausted");
  });

  it("fleets detail page pulses the WakePulse dot when the fleet is active", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    // mockFetchBilling returns a fleet with status "active" — exercises the
    // truthy arm of the status===ACTIVE ternary (renders <WakePulse live />).
    mockFetchBilling(happyBilling);
    const { default: Page } = await import("../app/(dashboard)/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toContain("data-live");
  });

  it("fleets detail page omits the WakePulse dot when the fleet is not active", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/approvals")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      if (url.includes("/events")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({
          // A paused fleet hits the null arm of the status===ACTIVE ternary —
          // no WakePulse is rendered, so the live dot is absent.
          items: [{ id: "zom_1", name: "platform-ops", status: "paused", created_at: 1, updated_at: 1 }],
          total: 1,
        }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toContain("paused");
    expect(markup).not.toContain("data-live");
  });

  it("fleets detail page renders pending-approvals badge + 50+ label when next_cursor set", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/approvals")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({
            items: [{ gate_id: "g1", fleet_id: "zom_1", fleet_name: "platform-ops" }],
            next_cursor: "cur_xyz",
          }),
        };
      }
      if (url.includes("/events")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({
          items: [{ id: "zom_1", name: "platform-ops", status: "active", created_at: 1, updated_at: 1 }],
          total: 1,
        }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toMatch(/1\+ pending approval/i);
    // Exactly one pending → singular ("") arm of the plural ternary.
    expect(markup).toContain("1+ pending approval");
    expect(markup).not.toMatch(/pending approvals/);
  });

  it("fleets detail page pluralizes the pending-approvals badge with more than one pending", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/approvals")) {
        return {
          ok: true,
          status: 200,
          // Two pending, no next_cursor → exact-count label "2" and the
          // plural "s" arm of the `length === 1 ? "" : "s"` ternary.
          json: async () => ({
            items: [
              { gate_id: "g1", fleet_id: "zom_1", fleet_name: "platform-ops" },
              { gate_id: "g2", fleet_id: "zom_1", fleet_name: "platform-ops" },
            ],
            next_cursor: null,
          }),
        };
      }
      if (url.includes("/events")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({
          items: [{ id: "zom_1", name: "platform-ops", status: "active", created_at: 1, updated_at: 1 }],
          total: 1,
        }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toContain("2 pending approvals");
  });

  it("fleets detail page handles billing fetch failure gracefully (catch branch)", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        throw new Error("network down");
      }
      if (url.includes("/approvals")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      if (url.includes("/events")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({
          items: [
            {
              id: "zom_1",
              name: "platform-ops",
              status: "active",
              created_at: 1713700000000,
              updated_at: 1713700000000,
            },
          ],
          total: 1,
        }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toContain("platform-ops");
    expect(markup).not.toContain("Balance exhausted");
  });

  // A still-provisioning fleet shows the install states on its own page (the
  // gate holds the panels until ready), with an installing indicator in the
  // header — so progress is never hidden, and "Open fleet" lands here while
  // installing and resolves in place.
  it("test_installing_fleet_always_visible — detail page shows install states + indicator while installing", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/approvals")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      if (url.includes("/events")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({
          items: [{ id: "zom_1", name: "fresh-bot", status: "installing", created_at: 1, updated_at: 1 }],
          total: 1,
        }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    // Header carries the status label + the installing live indicator.
    expect(markup).toContain("installing");
    expect(markup).toContain("data-live");
    // The install states surface is shown; the gate withholds the lower panels.
    expect(markup).toContain("Install states");
    expect(markup).not.toContain("Pending approvals");
  });

  it("fleets detail page degrades to empty when the events + approvals fetches fail (catch branches)", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/approvals")) throw new Error("approvals down");
      if (url.includes("/events")) throw new Error("events down");
      return {
        ok: true,
        status: 200,
        json: async () => ({
          items: [{ id: "zom_1", name: "platform-ops", status: "active", created_at: 1, updated_at: 1 }],
          total: 1,
        }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    // The fleet still renders; the failed events + approvals calls degrade
    // to empty via their `.catch` arms (the events list shows its empty state).
    expect(markup).toContain("platform-ops");
    expect(markup).toContain("No events yet");
  });
});

// TriggerPanel coverage moved to a co-located test file with the
// per-trigger accordion rewrite (`components/TriggerPanel.test.tsx`).
// The legacy Tabs interface tested in this block no longer exists.
