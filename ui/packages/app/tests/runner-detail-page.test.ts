import React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ApiError } from "@/lib/api/errors";

// ── Shared mocks (the runners-page harness shape: page guards under test,
// presentational children stubbed to markers — each child carries its own
// sibling test) ─────────────────────────────────────────────────────────────

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const notFound = vi.fn(() => {
  throw new Error("notFound");
});
const authMock = vi.fn();
const hasScopeMock = vi.fn();
const getRunnerMock = vi.fn();
const listRunnerLeasesMock = vi.fn();
const listRunnerEventsMock = vi.fn();

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

const NOT_ADMIN = "/settings?notice=runners-platform-admin-only";
const GRAFANA_ENV = "AGENTSFLEET_GRAFANA_BASE_URL";

const RUNNER = {
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

const EMPTY_PAGE = { items: [], total: 0, next_cursor: null };

function mockAuth(token: string | null = "tok") {
  authMock.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(token) });
}

function pageProps(query: Record<string, string | string[] | undefined> = {}) {
  return {
    params: Promise.resolve({ runnerId: RUNNER.id }),
    searchParams: Promise.resolve(query),
  };
}

async function loadPage() {
  const { default: Page } = await import(
    "../app/(dashboard)/admin/runners/[runnerId]/page"
  );
  return Page;
}

beforeEach(() => {
  vi.clearAllMocks();
  hasScopeMock.mockResolvedValue(true);
  delete process.env[GRAFANA_ENV];
});

describe("admin/runners/[runnerId] page", () => {
  it("redirects a caller without runner:read before any read happens", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow(`redirect:${NOT_ADMIN}`);
    expect(getRunnerMock).not.toHaveBeenCalled();
  });

  it("redirects to /sign-in when the admin session has no token", async () => {
    mockAuth(null);
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow("redirect:/sign-in");
  });

  it("renders notFound for an unknown runner id", async () => {
    mockAuth();
    getRunnerMock.mockRejectedValueOnce(new ApiError("no runner", 404, "UZ-RUN-014"));
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow("notFound");
  });

  it("redirects to settings when the backend independently 403s the read", async () => {
    mockAuth();
    getRunnerMock.mockRejectedValueOnce(new ApiError("forbidden", 403, "UZ-AUTH-022"));
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow(`redirect:${NOT_ADMIN}`);
  });

  it("redirects to /sign-in when the backend returns 401", async () => {
    mockAuth();
    getRunnerMock.mockRejectedValueOnce(new ApiError("expired", 401, "UZ-AUTH-401"));
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow("redirect:/sign-in");
  });

  it("re-throws a non-403/401 ApiError instead of redirecting", async () => {
    mockAuth();
    getRunnerMock.mockRejectedValueOnce(new ApiError("exploded", 500, "UZ-INTERNAL-001"));
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow("exploded");
  });

  it("lands on Leases by default: strip over the table, tracker armed, no Grafana without a base", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockResolvedValueOnce({
      items: [{ id: "lease-1" }],
      total: 1,
      next_cursor: null,
    });
    const Page = await loadPage();
    // No searchParams at all — the arm a bare route navigation takes.
    const html = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ runnerId: RUNNER.id }) }),
    );
    expect(html).toContain('data-runner-header="runner-prod-ams-01.internal"');
    expect(html).toContain('data-grafana="none"');
    expect(html).toContain('data-runner-rail="leases"');
    expect(html).toContain('data-runner-strip="1"');
    expect(html).toContain('data-lease-table="1"');
    expect(html).toContain('data-runner-viewed="active:busy"');
    expect(listRunnerLeasesMock).toHaveBeenCalledWith("tok", RUNNER.id, { limit: 25 });
    expect(listRunnerEventsMock).not.toHaveBeenCalled();
  });

  it("serves Activity with the lifecycle type set and no strip", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerEventsMock.mockResolvedValueOnce({
      items: [{ id: "evt-1" }, { id: "evt-2" }],
      total: 2,
      next_cursor: null,
    });
    const Page = await loadPage();
    const html = renderToStaticMarkup(await Page(pageProps({ view: "activity" })));
    expect(html).toContain('data-runner-rail="activity"');
    expect(html).toContain('data-activity-table="2"');
    expect(html).not.toContain("data-runner-strip");
    expect(listRunnerEventsMock).toHaveBeenCalledWith(
      "tok",
      RUNNER.id,
      expect.objectContaining({
        event_type:
          "runner_registered,runner_online,runner_offline,runner_cordoned,runner_draining,runner_drained,runner_revoked",
      }),
    );
    expect(listRunnerLeasesMock).not.toHaveBeenCalled();
  });

  it("forwards the cursor trail as starting_after on both views", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockResolvedValueOnce(EMPTY_PAGE);
    const Page = await loadPage();
    renderToStaticMarkup(await Page(pageProps({ c: "lease-cursor-1", cps: "25" })));
    expect(listRunnerLeasesMock).toHaveBeenCalledWith("tok", RUNNER.id, {
      limit: 25,
      starting_after: "lease-cursor-1",
    });

    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerEventsMock.mockResolvedValueOnce(EMPTY_PAGE);
    renderToStaticMarkup(
      await Page(pageProps({ view: "activity", c: "evt-cursor-1", cps: "25" })),
    );
    expect(listRunnerEventsMock).toHaveBeenCalledWith(
      "tok",
      RUNNER.id,
      expect.objectContaining({ starting_after: "evt-cursor-1" }),
    );
  });

  it("says the history is unavailable when a view read errors, never an empty history", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockRejectedValueOnce(new Error("lease read down"));
    const Page = await loadPage();
    const html = renderToStaticMarkup(await Page(pageProps()));
    // An empty table would read as "this host has never held a lease", which is
    // the opposite of what happened. The shell and the strip still render.
    expect(html).toContain("Lease history is temporarily unavailable");
    expect(html).not.toContain("data-lease-table");
    expect(html).toContain('data-runner-strip="1"');

    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerEventsMock.mockRejectedValueOnce(new Error("events read down"));
    const activityHtml = renderToStaticMarkup(await Page(pageProps({ view: "activity" })));
    expect(activityHtml).toContain("Activity history is temporarily unavailable");
    expect(activityHtml).not.toContain("data-activity-table");
  });

  it("builds the Grafana link only against a configured base, with the runner filter appended", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockResolvedValueOnce(EMPTY_PAGE);
    process.env[GRAFANA_ENV] = "https://grafana.example/d/runners";
    const Page = await loadPage();
    const html = renderToStaticMarkup(await Page(pageProps()));
    expect(html).toContain(
      `data-grafana="https://grafana.example/d/runners?var-runner_id=${RUNNER.id}"`,
    );

    // A base already carrying a query joins with & instead of a second ?.
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockResolvedValueOnce(EMPTY_PAGE);
    process.env[GRAFANA_ENV] = "https://grafana.example/d/runners?orgId=1";
    const withQuery = renderToStaticMarkup(await Page(pageProps()));
    // renderToStaticMarkup HTML-escapes the ampersand in the attribute.
    expect(withQuery).toContain(
      `data-grafana="https://grafana.example/d/runners?orgId=1&amp;var-runner_id=${RUNNER.id}"`,
    );
  });
});
