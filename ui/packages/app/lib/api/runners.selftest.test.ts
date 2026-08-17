import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AssignedPolicy, RunnerDetail, SelftestReport } from "./runners";

// The self-test slice of the runners client — the PATCH that records a request
// and the staleness rule the detail page renders from. Split from
// runners.test.ts (list/create/policy/events) to keep both inside the length cap.

const { requestMock } = vi.hoisted(() => ({ requestMock: vi.fn() }));
vi.mock("./client", () => ({ request: requestMock }));

import { isSelftestStale, requestRunnerSelftest } from "./runners";

beforeEach(() => vi.clearAllMocks());
afterEach(() => vi.resetAllMocks());

const ASSIGNED: AssignedPolicy = {
  sandbox_tier: "landlock_full",
  network_policy: "deny_all_egress",
  registry_allowlist: ["pypi.org"],
  worker_count: 2,
};

function report(overrides: Partial<SelftestReport> = {}): SelftestReport {
  return {
    checks: [{ name: "egress_blocked", ok: true, detail: "connect refused" }],
    all_ok: true,
    sandbox_tier: ASSIGNED.sandbox_tier,
    network_policy: ASSIGNED.network_policy,
    ...overrides,
  };
}

function detail(overrides: Partial<RunnerDetail> = {}): RunnerDetail {
  return {
    id: "runner-1",
    host_id: "runner-prod-ams-01.internal",
    sandbox_tier: "landlock_full",
    admin_state: "active",
    liveness: "online",
    labels: [],
    last_seen_at: 1_760_000_000_000,
    created_at: 1_759_000_000_000,
    assigned_policy: ASSIGNED,
    achievable: null,
    degraded: false,
    degraded_reason: null,
    selftest_requested_at: null,
    selftest_completed_at: 1_760_000_000_000,
    selftest: report(),
    active_lease_count: 0,
    active_fleet_count: 0,
    leases_acquired: 0,
    leases_succeeded: 0,
    leases_failed: 0,
    leases_expired: 0,
    ...overrides,
  };
}

describe("requestRunnerSelftest", () => {
  it("PATCHes the self_test action against the single-runner operator path", async () => {
    const recorded = { id: "runner-1", admin_state: "active", selftest_requested_at: 1_760_000_000_000 };
    requestMock.mockResolvedValueOnce(recorded);
    const r = await requestRunnerSelftest("tok", "runner-1");
    expect(r).toEqual(recorded);
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/fleets/runners/runner-1",
      { method: "PATCH", body: JSON.stringify({ action: "self_test" }) },
      "tok",
    );
  });

  it("percent-encodes the runner id rather than splicing it into the path raw", async () => {
    requestMock.mockResolvedValueOnce({ id: "a/b", admin_state: "active", selftest_requested_at: 1 });
    await requestRunnerSelftest("tok", "a/b");
    expect(requestMock).toHaveBeenCalledWith("/v1/fleets/runners/a%2Fb", expect.anything(), "tok");
  });

  it("propagates the daemon's refusal untouched — the caller renders the code", async () => {
    requestMock.mockRejectedValueOnce(new Error("UZ-RUN-018"));
    await expect(requestRunnerSelftest("tok", "runner-1")).rejects.toThrow("UZ-RUN-018");
  });
});

describe("isSelftestStale", () => {
  it("is not stale when no verdict exists — there is nothing to be stale about", () => {
    expect(isSelftestStale(detail({ selftest: null, selftest_completed_at: null }))).toBe(false);
  });

  it("is stale when a verdict outlives the assignment entirely (policy since unassigned)", () => {
    expect(isSelftestStale(detail({ assigned_policy: null }))).toBe(true);
  });

  it("is stale when the runner was re-tiered after the verdict landed", () => {
    expect(isSelftestStale(detail({ selftest: report({ sandbox_tier: "container_nested" }) }))).toBe(true);
  });

  it("is stale when only the network policy moved — a tier match alone does not prove the verdict", () => {
    expect(isSelftestStale(detail({ selftest: report({ network_policy: "allow_all" }) }))).toBe(true);
  });

  it("is current when the verdict names the tier and policy the runner still carries", () => {
    expect(isSelftestStale(detail())).toBe(false);
  });

  it("survives a row from a daemon older than these columns, which omits the keys entirely", () => {
    // The JSON round-trip drops undefined keys, reproducing the wire shape a
    // pre-selftest daemon sends. A strict `=== null` check falls through it and
    // dereferences undefined, which took the whole runner page down.
    const older = JSON.parse(
      JSON.stringify({ ...detail(), selftest: undefined, selftest_completed_at: undefined }),
    ) as RunnerDetail;
    expect(isSelftestStale(older)).toBe(false);
  });

  it("treats an omitted assignment as one the verdict can no longer describe", () => {
    const noPolicy = JSON.parse(JSON.stringify({ ...detail(), assigned_policy: undefined })) as RunnerDetail;
    expect(isSelftestStale(noPolicy)).toBe(true);
  });

  it("reads staleness off the verdict, not off the runner's own tier column", () => {
    // The row's `sandbox_tier` is the live column; the comparison must use the
    // tier recorded WITH the verdict, or a re-tier would look freshly proven.
    const stale = detail({
      sandbox_tier: "container_nested",
      assigned_policy: { ...ASSIGNED, sandbox_tier: "container_nested" },
      selftest: report({ sandbox_tier: "landlock_full" }),
    });
    expect(isSelftestStale(stale)).toBe(true);
  });
});
