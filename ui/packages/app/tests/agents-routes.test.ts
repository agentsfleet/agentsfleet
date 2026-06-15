import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { NANOS_PER_USD } from "@/lib/types";
import { resolveActiveWorkspace, fetchMock, resetCommonMocks, authMock as auth } from "./helpers/dashboard-mocks";

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
vi.mock("@/components/domain/AgentApprovalsPanel", () => ({
  default: () => React.createElement("div", { "data-stub": "AgentApprovalsPanel" }),
}));
vi.mock("lucide-react", async () => (await import("./helpers/dashboard-mocks")).lucideMock());
vi.mock("@agentsfleet/design-system", async (orig) => {
  const h = await import("./helpers/dashboard-mocks");
  return { ...h.designSystemCore(await orig<Record<string, unknown>>()), ...h.designSystemTabs() };
});

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks({ pathname: "/agents" });
});
afterEach(() => {
  cleanup();
  fetchMock.mockReset();
});

// ── Agents route — page, loading, detail, new ─────────────────────────────

describe("agents routes", () => {
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
    const { default: Loading } = await import("../app/(dashboard)/agents/loading");
    render(React.createElement(Loading));
    const el = screen.getByRole("status");
    expect(el.textContent).toContain("Loading agents");
    // Branded WakePulse dot (data-live), not the off-system Loader2Icon spin.
    const dot = el.querySelector("[data-live]");
    expect(dot).toBeTruthy();
    expect(dot?.className).toContain("bg-pulse");
  });

  it("agents list page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/agents/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("agents list page renders empty-workspace state", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce(null);
    const { default: Page } = await import("../app/(dashboard)/agents/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("No workspace yet");
  });

  it("agents list page renders empty-agents state with banner suppressed", async () => {
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
    const { default: Page } = await import("../app/(dashboard)/agents/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("No agents yet");
    expect(markup).toContain("Install Agent");
    expect(markup).not.toContain("credit balance is exhausted");
  });

  it("agents list page renders populated list + exhaustion banner", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    mockFetchBilling(exhaustedBilling);
    const { default: Page } = await import("../app/(dashboard)/agents/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("href=\"/agents/zom_1\"");
    expect(markup).toContain("platform-ops");
    expect(markup).toContain("credit balance is exhausted");
  });

  it("agents list page swallows a failed billing fetch and still renders", async () => {
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
    const { default: Page } = await import("../app/(dashboard)/agents/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("No agents yet");
  });

  it("agents new page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/agents/new/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("agents new page renders empty-workspace guard", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce(null);
    const { default: Page } = await import("../app/(dashboard)/agents/new/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("Create a workspace before installing agents");
  });

  it("agents new page renders the install form when a workspace exists", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    const { default: Page } = await import("../app/(dashboard)/agents/new/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("Install Agent");
    expect(markup).toContain("name=\"trigger_markdown\"");
    expect(markup).toContain("name=\"source_markdown\"");
  });

  it("agents detail page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/agents/[id]/page");
    await expect(Page({ params: Promise.resolve({ id: "zom_1" }) })).rejects.toThrow(
      "redirect:/sign-in",
    );
  });

  it("agents detail page notFound when no workspace", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce(null);
    const { default: Page } = await import("../app/(dashboard)/agents/[id]/page");
    await expect(Page({ params: Promise.resolve({ id: "zom_1" }) })).rejects.toThrow(
      "notFound",
    );
  });

  it("agents detail page notFound when agent id is not in the list", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    mockFetchBilling(happyBilling);
    const { default: Page } = await import("../app/(dashboard)/agents/[id]/page");
    await expect(
      Page({ params: Promise.resolve({ id: "missing" }) }),
    ).rejects.toThrow("notFound");
  });

  it("agents detail page renders panels + exhaustion badge when tenant is exhausted", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    mockFetchBilling(exhaustedBilling);
    const { default: Page } = await import("../app/(dashboard)/agents/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toContain("platform-ops");
    expect(markup).toContain("Trigger");
    expect(markup).toContain("Configuration");
    expect(markup).toContain("Balance exhausted");
  });

  it("agents detail page renders without badge when not exhausted", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    mockFetchBilling(happyBilling);
    const { default: Page } = await import("../app/(dashboard)/agents/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).not.toContain("Balance exhausted");
  });

  it("agents detail page pulses the WakePulse dot when the agent is active", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    // mockFetchBilling returns a agent with status "active" — exercises the
    // truthy arm of the status===ACTIVE ternary (renders <WakePulse live />).
    mockFetchBilling(happyBilling);
    const { default: Page } = await import("../app/(dashboard)/agents/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toContain("data-live");
  });

  it("agents detail page omits the WakePulse dot when the agent is not active", async () => {
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
          // A paused agent hits the null arm of the status===ACTIVE ternary —
          // no WakePulse is rendered, so the live dot is absent.
          items: [{ id: "zom_1", name: "platform-ops", status: "paused", created_at: 1, updated_at: 1 }],
          total: 1,
        }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/agents/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toContain("paused");
    expect(markup).not.toContain("data-live");
  });

  it("agents detail page renders pending-approvals badge + 50+ label when next_cursor set", async () => {
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
            items: [{ gate_id: "g1", agent_id: "zom_1", agent_name: "platform-ops" }],
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
    const { default: Page } = await import("../app/(dashboard)/agents/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toMatch(/1\+ pending approval/i);
    // Exactly one pending → singular ("") arm of the plural ternary.
    expect(markup).toContain("1+ pending approval");
    expect(markup).not.toMatch(/pending approvals/);
  });

  it("agents detail page pluralizes the pending-approvals badge with more than one pending", async () => {
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
              { gate_id: "g1", agent_id: "zom_1", agent_name: "platform-ops" },
              { gate_id: "g2", agent_id: "zom_1", agent_name: "platform-ops" },
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
    const { default: Page } = await import("../app/(dashboard)/agents/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toContain("2 pending approvals");
  });

  it("agents detail page handles billing fetch failure gracefully (catch branch)", async () => {
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
    const { default: Page } = await import("../app/(dashboard)/agents/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toContain("platform-ops");
    expect(markup).not.toContain("Balance exhausted");
  });

  it("agents detail page degrades to empty when the events + approvals fetches fail (catch branches)", async () => {
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
    const { default: Page } = await import("../app/(dashboard)/agents/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    // The agent still renders; the failed events + approvals calls degrade
    // to empty via their `.catch` arms (the events list shows its empty state).
    expect(markup).toContain("platform-ops");
    expect(markup).toContain("No events yet");
  });
});

// TriggerPanel coverage moved to a co-located test file with the
// per-trigger accordion rewrite (`components/TriggerPanel.test.tsx`).
// The legacy Tabs UI tested in this block no longer exists.

