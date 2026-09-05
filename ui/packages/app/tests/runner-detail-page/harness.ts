import React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ApiError } from "@/lib/api/errors";

// ── Shared mocks (the runners-page harness shape: page guards under test,
// presentational children stubbed to markers — each child carries its own
// sibling test) ─────────────────────────────────────────────────────────────

export const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
export const notFound = vi.fn(() => {
  throw new Error("notFound");
});
export const authMock = vi.fn();
export const hasScopeMock = vi.fn();
export const getRunnerMock = vi.fn();
export const listRunnerLeasesMock = vi.fn();
export const listRunnerEventsMock = vi.fn();

vi.mock("next/navigation", () => ({ redirect, notFound }));
vi.mock("@clerk/nextjs/server", () => ({ auth: authMock }));
vi.mock("@/lib/auth/platform", () => ({ hasScope: hasScopeMock }));

vi.mock("@/lib/api/runners", async (orig) => ({
  ...(await orig<typeof import("@/lib/api/runners")>()),
  getRunner: getRunnerMock,
  listRunnerLeases: listRunnerLeasesMock,
  listRunnerEvents: listRunnerEventsMock,
}));

vi.mock(
  "@/app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader",
  () => ({
    RunnerHeader: ({ runner, grafanaHref }: { runner: { host_id: string }; grafanaHref: string | null }) =>
      React.createElement("div", { "data-runner-header": runner.host_id, "data-grafana": grafanaHref ?? "none" }),
  }),
);
vi.mock(
  "@/app/(dashboard)/admin/runners/[runnerId]/components/RunnerSubnavigation",
  () => ({
    RunnerSubnavigation: ({ activeView }: { activeView: string }) =>
      React.createElement("div", { "data-runner-rail": activeView }),
  }),
);
vi.mock(
  "@/app/(dashboard)/admin/runners/[runnerId]/components/RunnerMetricsStrip",
  () => ({
    default: () => React.createElement("div", { "data-runner-strip": "1" }),
  }),
);
vi.mock(
  "@/app/(dashboard)/admin/runners/[runnerId]/components/LeaseTable",
  () => ({
    LeaseTable: ({ initial }: { initial: { items: unknown[] } }) =>
      React.createElement("div", { "data-lease-table": String(initial.items.length) }),
  }),
);
vi.mock(
  "@/app/(dashboard)/admin/runners/[runnerId]/components/ActivityTable",
  () => ({
    ActivityTable: ({ initial }: { initial: { items: unknown[] } }) =>
      React.createElement("div", { "data-activity-table": String(initial.items.length) }),
  }),
);
vi.mock(
  "@/app/(dashboard)/admin/runners/[runnerId]/components/RunnerViewedTracker",
  () => ({
    RunnerViewedTracker: ({ liveness, adminState }: { liveness: string; adminState: string }) =>
      React.createElement("div", { "data-runner-viewed": `${adminState}:${liveness}` }),
  }),
);

export const NOT_ADMIN = "/settings?notice=runners-platform-admin-only";
export const GRAFANA_ENV = "AGENTSFLEET_GRAFANA_BASE_URL";

export const RUNNER = {
  id: "01J2WQ8F3K7VZ9XB4N6MTYD5AR",
  host_id: "runner-prod-ams-01.internal",
  sandbox_tier: "landlock_full",
  admin_state: "active",
  liveness: "busy",
  labels: ["gpu"],
  last_seen_at: 10,
  created_at: 1,
  active_lease_count: 2,
  active_fleet_count: 2,
  leases_acquired: 7,
  leases_succeeded: 4,
  leases_failed: 1,
  leases_expired: 2,
};

export const EMPTY_PAGE = { items: [], total: 0, next_cursor: null };

export function mockAuth(token: string | null = "tok") {
  authMock.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(token) });
}

export function pageProps(query: Record<string, string | string[] | undefined> = {}) {
  return {
    params: Promise.resolve({ runnerId: RUNNER.id }),
    searchParams: Promise.resolve(query),
  };
}

export async function loadPage() {
  const { default: Page } = await import(
    "../../app/(dashboard)/admin/runners/[runnerId]/page"
  );
  return Page;
}

beforeEach(() => {
  vi.clearAllMocks();
  hasScopeMock.mockResolvedValue(true);
  // The view read now starts beside the runner read, so it is issued even when
  // the runner read ends in a redirect or not-found. It must answer with a
  // promise in every case; tests that care queue their own value first.
  listRunnerLeasesMock.mockResolvedValue(EMPTY_PAGE);
  listRunnerEventsMock.mockResolvedValue(EMPTY_PAGE);
  delete process.env[GRAFANA_ENV];
});
