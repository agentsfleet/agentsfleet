import { FLEET_ID, FLEET_DETAIL, response } from "../helpers/draft-fixtures";
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
  balance_nanos: string;
  updated_at: string;
  is_exhausted: boolean;
  exhausted_at: string | null;
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
  balance_nanos: String(NANOS_PER_USD),
  updated_at: "1970-01-01T00:00:00Z",
  is_exhausted: false,
  exhausted_at: null,
};
export const exhaustedBilling: BillingSnapshot = {
  ...happyBilling,
  is_exhausted: true,
  exhausted_at: "1970-01-01T00:00:00.001Z",
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

// Detail routes share the native saved-fleet shape, with per-test overrides.
export function detailBody(over: Record<string, unknown> = {}) {
  return {
    ...FLEET_DETAIL,
    name: "platform-ops",
    description: FLEET_DETAIL.definition.description,
    status: "active",
    events_processed: "0",
    budget_used_nanos: "0",
    pending_approvals: 0,
    created_at: "1970-01-01T00:00:00.001Z",
    updated_at: "1970-01-01T00:00:00.001Z",
    ...over,
  };
}

export function detailResponse(over: Record<string, unknown> = {}) {
  return response(detailBody(over), 200, {ETag: FLEET_DETAIL.etag});
}

export function mockFetchBilling(billing: BillingSnapshot, {missingStatus = 404, fleet = {}}: {
  missingStatus?: number; fleet?: Record<string, unknown>;
} = {}) {
  const respond = async (url: string): Promise<Response> => {
    if (url.endsWith("/v1/tenants/me/billing")) {
      return response(billing);
    }
    if (url.includes("/approvals") || url.includes("/events")) {
      return response({items: [], next_cursor: null});
    }
    if (url.includes("/memories")) {
      return response({items: [], total: 0, next_cursor: null});
    }
    if (url.includes("/messages")) {
      return response({items: [], total: null, next_cursor: null});
    }
    const detailMatch = url.match(/\/fleets\/([^/]+)$/);
    if (detailMatch) {
      if (detailMatch[1] !== FLEET_ID) {
        return response({error_code: "UZ-AGT-009", detail: "Fleet not found"}, missingStatus);
      }
      return detailResponse(fleet);
    }
    return response({items: [detailBody(fleet)], total: null, next_cursor: null});
  };
  fetchMock.mockImplementation(respond);
  return respond;
}
