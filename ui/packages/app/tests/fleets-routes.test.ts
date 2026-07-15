import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { NANOS_PER_USD } from "@/lib/types";
import { fetchMock, resetCommonMocks, authMock as auth } from "./helpers/dashboard-mocks";
import { listWorkspaceFleetLibraryMock, listSecretsMock } from "./helpers/dashboard-app-mocks";

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
vi.mock("@/lib/api/fleet-library", async () => (await import("./helpers/dashboard-app-mocks")).fleetLibraryMock());
vi.mock("@/lib/api/secrets", async () => (await import("./helpers/dashboard-app-mocks")).secretsApiMock());

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks({ pathname: "/w/ws_1/fleets" });
  // The /fleets/new install page fetches the template gallery; default it to an
  // empty catalog so tests that don't care about templates don't crash on the
  // unmocked promise (individual tests override as needed). The Fleets list
  // empty-state no longer fetches — it routes to /fleets/new instead.
  listWorkspaceFleetLibraryMock.mockResolvedValue({ items: [] });
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
  const SAMPLE_TEMPLATES = [
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
  ];

  // The single-fleet detail body getFleet now reads (M131 §1) — the fields the
  // detail page renders. Inline fetch mocks return this for `…/fleets/{id}`
  // instead of the old list envelope the list-scan getFleet used to page.
  function detailBody(over: Record<string, unknown> = {}) {
    return {
      id: "zom_1",
      name: "platform-ops",
      status: "active",
      source_markdown: "# SKILL",
      trigger_markdown: null,
      bundle_content_hash: null,
      triggers: null,
      events_processed: 0,
      budget_used_nanos: 0,
      created_at: 1,
      updated_at: 1,
      ...over,
    };
  }

  function mockFetchBilling(billing: BillingSnapshot) {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => billing };
      }
      if (url.includes("/approvals")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      if (url.includes("/memories")) {
        return { ok: true, status: 200, json: async () => ({ items: [], total: 0, request_id: "req_1" }) };
      }
      if (url.includes("/events")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      // The single-fleet detail read (M131 §1): `…/fleets/{id}` with a trailing
      // id segment. getFleet reads the fleet object directly (not a list scan); a
      // fleet id other than the seeded one is a 404, which getFleet throws and
      // the page maps to notFound(). The bare `…/fleets` list URL falls through
      // to the list envelope below.
      const detailMatch = url.match(/\/fleets\/([^/]+)$/);
      if (detailMatch) {
        if (detailMatch[1] !== "zom_1") {
          return {
            ok: false,
            status: 404,
            headers: { get: () => null },
            json: async () => ({ error_code: "UZ-AGT-009", detail: "Fleet not found" }),
          };
        }
        return {
          ok: true,
          status: 200,
          headers: { get: (k: string) => (k.toLowerCase() === "etag" ? '"seed-etag"' : null) },
          json: async () => detailBody(),
        };
      }
      return { ok: true, status: 200, json: async () => ({ items: [detailBody()], total: 1 }) };
    });
  }

  it("loading.tsx renders a spinner with status role", async () => {
    const { default: Loading } = await import("../app/(dashboard)/w/[workspaceId]/fleets/loading");
    render(React.createElement(Loading));
    const el = screen.getByRole("status");
    expect(el.textContent).toContain("Loading Fleets");
    // Branded WakePulse dot (data-live), not the off-system Loader2Icon spin.
    const dot = el.querySelector("[data-live]");
    expect(dot).toBeTruthy();
    expect(dot?.className).toContain("bg-pulse");
  });

  it("fleets list page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/page");
    await expect(Page({ params: Promise.resolve({ workspaceId: "ws_1" }) })).rejects.toThrow(
      "redirect:/sign-in",
    );
  });

  it("fleets list page shell streams the header + skeleton before data", async () => {
    // The shell paints the header synchronously; FleetsData is an async child,
    // so renderToStaticMarkup renders the Suspense skeleton in its place and the
    // data content stays absent until it streams in.
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/page");
    const markup = renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }));
    expect(markup).toContain("Fleets"); // PageTitle in the shell
    expect(markup).toContain("animate-pulse"); // Skeleton fallback
    expect(markup).not.toContain("platform-ops"); // data not yet resolved
  });

  it("FleetsData returns null when the token is missing", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { FleetsData } = await import("../app/(dashboard)/w/[workspaceId]/fleets/page");
    expect(await FleetsData({ workspaceId: "ws_1" })).toBeNull();
  });

  it("fleets list page renders the empty-fleets state (centered EmptyState), banner suppressed", async () => {
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
    const { FleetsData } = await import("../app/(dashboard)/w/[workspaceId]/fleets/page");
    const markup = renderToStaticMarkup(React.createElement(React.Fragment, null, await FleetsData({ workspaceId: "ws_1" })));
    expect(markup).toContain("No fleets yet");
    // The empty state is a centered EmptyState that routes to /w/ws_1/fleets/new
    // (where the template gallery lives) — no inline gallery, no quickstart here.
    // The action pair is [Learn more] + [Install fleet]; template authoring lives
    // on the install page, not here.
    expect(markup).toContain('href="/w/ws_1/fleets/new"');
    expect(markup).toContain("Install fleet");
    expect(markup).toContain("Learn more");
    expect(markup).not.toContain("Create fleet library");
    expect(markup).not.toContain("Quick start");
    expect(markup).not.toContain("?library=");
    expect(markup).not.toContain("credit balance is exhausted");
  });

  it("fleets list page renders populated list + exhaustion banner", async () => {
    mockFetchBilling(exhaustedBilling);
    const { FleetsData } = await import("../app/(dashboard)/w/[workspaceId]/fleets/page");
    const markup = renderToStaticMarkup(React.createElement(React.Fragment, null, await FleetsData({ workspaceId: "ws_1" })));
    expect(markup).toContain("href=\"/w/ws_1/fleets/zom_1\"");
    expect(markup).toContain("platform-ops");
    expect(markup).toContain("credit balance is exhausted");
  });

  it("fleets list page swallows a failed billing fetch and still renders", async () => {
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
    const { FleetsData } = await import("../app/(dashboard)/w/[workspaceId]/fleets/page");
    const markup = renderToStaticMarkup(React.createElement(React.Fragment, null, await FleetsData({ workspaceId: "ws_1" })));
    expect(markup).toContain("No fleets yet");
  });

  it("fleets new page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1" }), searchParams: Promise.resolve({}) }),
    ).rejects.toThrow("redirect:/sign-in");
  });

  it("fleets new page renders the gallery-first install flow when a workspace exists", async () => {
    listWorkspaceFleetLibraryMock.mockResolvedValue({ items: SAMPLE_TEMPLATES });
    listSecretsMock.mockResolvedValue({ secrets: [{ kind: "custom_secret", name: "github", created_at: 1 }] });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1" }), searchParams: Promise.resolve({}) }),
    );
    expect(markup).toContain("Install fleet"); // page title
    expect(markup).toContain("Fleet library");
    expect(markup).toContain("GitHub PR reviewer");
    expect(markup).toContain("Use entry"); // the gallery card's install action
  });

  it("fleets new page swallows failed template + secret fetches", async () => {
    listWorkspaceFleetLibraryMock.mockRejectedValue(new Error("catalog down"));
    listSecretsMock.mockRejectedValue(new Error("vault down"));
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1" }), searchParams: Promise.resolve({}) }),
    );
    expect(markup).toContain("No prebuilt fleet library found"); // empty gallery
  });

  it("fleets new page accepts a ?library= deep link", async () => {
    listWorkspaceFleetLibraryMock.mockResolvedValue({ items: [] });
    listSecretsMock.mockResolvedValue({ secrets: [] });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1" }),
        searchParams: Promise.resolve({ library: "github-pr-reviewer" }),
      }),
    );
    expect(markup).toContain("Fleet library");
  });

  it("fleets detail page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }) }),
    ).rejects.toThrow("redirect:/sign-in");
  });

  it("fleets detail page notFound when fleet id is not in the list", async () => {
    mockFetchBilling(happyBilling);
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "missing" }) }),
    ).rejects.toThrow("notFound");
  });

  it("fleets detail page renders panels + exhaustion badge when tenant is exhausted", async () => {
    mockFetchBilling(exhaustedBilling);
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }) }),
    );
    expect(markup).toContain("platform-ops");
    expect(markup).toContain("Balance exhausted");
  });

  it("test_console_renders_three_columns", async () => {
    // The three-column console (M131 §3): what the fleet IS / DOES / KNOWS &
    // COSTS. Each is a labelled region carrying its panels (the source editor,
    // the metrics strip, the memory panel, the runs ledger).
    mockFetchBilling(happyBilling);
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }) }),
    );
    // Ampersand escapes to &amp; in static markup, so assert the ampersand-free
    // heads of each column label.
    expect(markup).toContain("What it is");
    expect(markup).toContain("What it does");
    expect(markup).toContain("What it knows");
    // The left rail's source editor + the right rail's memory and runs panels.
    expect(markup).toContain("Source");
    expect(markup).toContain("Memory");
    expect(markup).toContain("Runs");
  });

  it("fleets detail page renders without badge when not exhausted", async () => {
    mockFetchBilling(happyBilling);
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }) }),
    );
    expect(markup).not.toContain("Balance exhausted");
  });

  it("fleets detail page pulses the WakePulse dot when the fleet is active", async () => {
    // mockFetchBilling returns a fleet with status "active" — exercises the
    // truthy arm of the status===ACTIVE ternary (renders <WakePulse live />).
    mockFetchBilling(happyBilling);
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }) }),
    );
    expect(markup).toContain("data-live");
  });

  it("fleets detail page omits the WakePulse dot when the fleet is not active", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/approvals")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      if (url.includes("/memories")) {
        return { ok: true, status: 200, json: async () => ({ items: [], total: 0, request_id: "req_1" }) };
      }
      if (url.includes("/events")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => detailBody({ name: "platform-ops", status: "paused" }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }) }),
    );
    expect(markup).toContain("paused");
    expect(markup).not.toContain("data-live");
  });

  it("fleets detail page renders pending-approvals badge + 50+ label when next_cursor set", async () => {
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
      if (url.includes("/memories")) {
        return { ok: true, status: 200, json: async () => ({ items: [], total: 0, request_id: "req_1" }) };
      }
      if (url.includes("/events")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => detailBody({ name: "platform-ops", status: "active" }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }) }),
    );
    expect(markup).toMatch(/1\+ pending approval/i);
    // Exactly one pending → singular ("") arm of the plural ternary.
    expect(markup).toContain("1+ pending approval");
    expect(markup).not.toMatch(/pending approvals/);
  });

  it("fleets detail page pluralizes the pending-approvals badge with more than one pending", async () => {
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
      if (url.includes("/memories")) {
        return { ok: true, status: 200, json: async () => ({ items: [], total: 0, request_id: "req_1" }) };
      }
      if (url.includes("/events")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => detailBody({ name: "platform-ops", status: "active" }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }) }),
    );
    expect(markup).toContain("2 pending approvals");
  });

  it("fleets detail page handles billing fetch failure gracefully (catch branch)", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        throw new Error("network down");
      }
      if (url.includes("/approvals")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      if (url.includes("/memories")) {
        return { ok: true, status: 200, json: async () => ({ items: [], total: 0, request_id: "req_1" }) };
      }
      if (url.includes("/events")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      return { ok: true, status: 200, json: async () => detailBody() };
    });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }) }),
    );
    expect(markup).toContain("platform-ops");
    expect(markup).not.toContain("Balance exhausted");
  });

  // A still-provisioning fleet shows the install states on its own page (the
  // gate holds the panels until ready), with an installing indicator in the
  // header — so progress is never hidden, and "Open fleet" lands here while
  // installing and resolves in place.
  it("test_installing_fleet_always_visible — detail page shows install states + indicator while installing", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/approvals")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      if (url.includes("/memories")) {
        return { ok: true, status: 200, json: async () => ({ items: [], total: 0, request_id: "req_1" }) };
      }
      if (url.includes("/events")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => detailBody({ name: "fresh-bot", status: "installing" }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }) }),
    );
    // Header carries the status label + the installing live indicator.
    expect(markup).toContain("installing");
    expect(markup).toContain("data-live");
    // The install states surface is shown; the gate withholds the lower panels.
    expect(markup).toContain("Install states");
    expect(markup).not.toContain("Pending approvals");
  });

  it("fleets detail page degrades to empty when the events + approvals fetches fail (catch branches)", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/approvals")) throw new Error("approvals down");
      if (url.includes("/memories")) throw new Error("memories down");
      if (url.includes("/events")) throw new Error("events down");
      return {
        ok: true,
        status: 200,
        json: async () => detailBody({ name: "platform-ops", status: "active" }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }) }),
    );
    // The fleet still renders; the failed events + approvals calls degrade via
    // their `.catch` arms. The 7-day window catches to `null`, so the runs
    // ledger shows its degraded state rather than a blank, and the metrics strip
    // shows no run.
    expect(markup).toContain("platform-ops");
    expect(markup).toContain("Recent window unavailable");
    expect(markup).toContain("No runs recorded yet");
  });
});

// TriggerPanel coverage moved to a co-located test file with the
// per-trigger accordion rewrite (`components/TriggerPanel.test.tsx`).
// The legacy Tabs interface tested in this block no longer exists.
