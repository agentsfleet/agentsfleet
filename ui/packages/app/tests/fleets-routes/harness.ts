import { afterEach, beforeEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import { NANOS_PER_USD } from "@/lib/types";
import {
  fetchMock,
  resetCommonMocks,
  authMock as auth,
} from "../helpers/dashboard-mocks";
import {
  listWorkspaceFleetLibraryMock,
  listSecretsMock,
} from "../helpers/dashboard-app-mocks";

export type BillingSnapshot = {
  balance_nanos: number;
  updated_at: number;
  is_exhausted: boolean;
  exhausted_at: number | null;
};

// Shared dashboard mock harness — see tests/helpers/dashboard-mocks.tsx.
vi.stubGlobal("fetch", fetchMock);
vi.mock("next/navigation", async () =>
  (await import("../helpers/dashboard-mocks")).nextNavigationMock(),
);
vi.mock("@clerk/nextjs/server", async () =>
  (await import("../helpers/dashboard-mocks")).clerkServerMock(),
);
vi.mock("@clerk/nextjs", async () =>
  (await import("../helpers/dashboard-mocks")).clerkMock(),
);
vi.mock("next/link", async () =>
  (await import("../helpers/dashboard-mocks")).nextLinkMock(),
);
vi.mock("@/lib/workspace", async () =>
  (await import("../helpers/dashboard-mocks")).workspaceMock(),
);
vi.mock("lucide-react", async () =>
  (await import("../helpers/dashboard-mocks")).lucideMock(),
);
vi.mock("@agentsfleet/design-system", async (orig) => {
  const h = await import("../helpers/dashboard-mocks");
  return {
    ...h.designSystemCore(await orig<Record<string, unknown>>()),
    ...h.designSystemTabs(),
  };
});
vi.mock("@/lib/api/fleet-library", async () =>
  (await import("../helpers/dashboard-app-mocks")).fleetLibraryMock(),
);
vi.mock("@/lib/api/secrets", async () =>
  (await import("../helpers/dashboard-app-mocks")).secretsApiMock(),
);

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

// ── Fixtures ──────────────────────────────────────────────────────────────

export const happyBilling: BillingSnapshot = {
  balance_nanos: NANOS_PER_USD,
  updated_at: 0,
  is_exhausted: false,
  exhausted_at: null,
};
export const exhaustedBilling: BillingSnapshot = {
  ...happyBilling,
  is_exhausted: true,
  exhausted_at: 1,
};
export const SAMPLE_TEMPLATES = [
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
  },
];

// The single-fleet detail body getFleet now reads (M131 §1) — the fields the
// detail page renders. Inline fetch mocks return this for `…/fleets/{id}`
// instead of the old list envelope the list-scan getFleet used to page.
export function detailBody(over: Record<string, unknown> = {}) {
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
    pending_approvals: 0,
    created_at: 1,
    updated_at: 1,
    ...over,
  };
}

export function detailResponse(over: Record<string, unknown> = {}) {
  return {
    ok: true,
    status: 200,
    headers: {
      get: (key: string) =>
        key.toLowerCase() === "etag" ? '"seed-etag"' : null,
    },
    json: async () => detailBody(over),
  };
}

export function mockFetchBilling(billing: BillingSnapshot) {
  fetchMock.mockImplementation(async (url: string) => {
    if (url.endsWith("/v1/tenants/me/billing")) {
      return { ok: true, status: 200, json: async () => billing };
    }
    if (url.includes("/approvals")) {
      return {
        ok: true,
        status: 200,
        json: async () => ({ items: [], next_cursor: null }),
      };
    }
    if (url.includes("/memories")) {
      return {
        ok: true,
        status: 200,
        json: async () => ({ items: [], total: 0, next_cursor: null }),
      };
    }
    // The chat's thread read: a real (empty) thread page, not the list
    // envelope the fall-through returns — the strip builds its summary from
    // this shape and rightly refuses one without `items`.
    if (url.includes("/messages")) {
      return {
        ok: true,
        status: 200,
        json: async () => ({ items: [], total: null, next_cursor: null }),
      };
    }
    if (url.includes("/events")) {
      return {
        ok: true,
        status: 200,
        json: async () => ({ items: [], next_cursor: null }),
      };
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
          json: async () => ({
            error_code: "UZ-AGT-009",
            detail: "Fleet not found",
          }),
        };
      }
      return detailResponse();
    }
    return {
      ok: true,
      status: 200,
      json: async () => ({ items: [detailBody()], total: 1 }),
    };
  });
}
