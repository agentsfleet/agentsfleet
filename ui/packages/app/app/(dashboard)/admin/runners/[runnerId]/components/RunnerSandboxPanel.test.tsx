import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import type { AssignedPolicy, RunnerDetail, SelftestReport } from "@/lib/api/runners";
import { RunnerSandboxPanel } from "./RunnerSandboxPanel";

afterEach(() => cleanup());

const ASSIGNED: AssignedPolicy = {
  sandbox_tier: "landlock_full",
  network_policy: "deny_all_egress",
  registry_allowlist: [],
  worker_count: 2,
};

const PASSING: SelftestReport = {
  checks: [
    { name: "resolver file resolves inside the sandbox", ok: true, detail: "no fault detected" },
    { name: "a hostname resolves inside the sandbox", ok: true, detail: "no fault detected" },
  ],
  all_ok: true,
  sandbox_tier: ASSIGNED.sandbox_tier,
  network_policy: ASSIGNED.network_policy,
};

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
    selftest_completed_at: null,
    selftest: null,
    active_lease_count: 0,
    active_fleet_count: 0,
    leases_acquired: 0,
    leases_succeeded: 0,
    leases_failed: 0,
    leases_expired: 0,
    ...overrides,
  };
}

function panel() {
  return screen.getByRole("article", { name: "Sandbox" });
}

describe("RunnerSandboxPanel — self-test half", () => {
  it("says the runner has never been tested rather than showing an empty verdict", () => {
    render(<RunnerSandboxPanel runner={detail()} />);
    expect(within(panel()).getByText(/Never self-tested/)).toBeTruthy();
    expect(screen.queryByText("all checks passed")).toBeNull();
  });

  it("renders a row from a daemon older than these columns instead of throwing on it", () => {
    // The JSON round-trip drops undefined keys — the wire shape a pre-selftest
    // daemon sends. The panel reads it as never-tested, not as a crash.
    const older = JSON.parse(
      JSON.stringify({
        ...detail(),
        selftest: undefined,
        selftest_completed_at: undefined,
        selftest_requested_at: undefined,
      }),
    ) as RunnerDetail;
    render(<RunnerSandboxPanel runner={older} />);
    expect(within(panel()).getByText(/Never self-tested/)).toBeTruthy();
    expect(within(panel()).queryByText(/Invalid Date/)).toBeNull();
  });

  it("names an outstanding request so a blank verdict does not read as a healthy one", () => {
    render(<RunnerSandboxPanel runner={detail({ selftest_requested_at: 1_760_000_000_000 })} />);
    expect(within(panel()).getByText(/A self-test is outstanding/)).toBeTruthy();
  });

  // Dimension 1.2 — each check the host reported gets its own name and detail
  // line, so "DNS failed inside the sandbox" reads without a journal.
  it("test_selftest_result_renders_per_check", () => {
    render(
      <RunnerSandboxPanel
        runner={detail({ selftest: PASSING, selftest_completed_at: 1_760_000_000_000 })}
      />,
    );
    expect(within(panel()).getByText("all checks passed")).toBeTruthy();
    expect(within(panel()).getByText("resolver file resolves inside the sandbox")).toBeTruthy();
    expect(within(panel()).getAllByText("no fault detected")).toHaveLength(2);
  });

  it("counts the failures rather than reporting one aggregate verdict", () => {
    const failing: SelftestReport = {
      ...PASSING,
      all_ok: false,
      checks: [
        { name: "resolver file resolves inside the sandbox", ok: false, detail: "the stub is not bound" },
        { name: "a hostname resolves inside the sandbox", ok: false, detail: "the resolver did not answer" },
        { name: "the inference endpoint is reachable", ok: true, detail: "no fault detected" },
      ],
    };
    render(<RunnerSandboxPanel runner={detail({ selftest: failing, selftest_completed_at: 1 })} />);
    expect(within(panel()).getByText("2 failed")).toBeTruthy();
    expect(within(panel()).getByText("the stub is not bound")).toBeTruthy();
  });

  // Dimension 1.3 — the result is history, and the page has to say so or an
  // operator reads a passing verdict as proof of the current policy.
  it("test_stale_selftest_result_is_labelled", () => {
    render(
      <RunnerSandboxPanel
        runner={detail({
          selftest: { ...PASSING, network_policy: "allow_all" },
          selftest_completed_at: 1,
        })}
      />,
    );
    expect(within(panel()).getByText("stale")).toBeTruthy();
    expect(within(panel()).getByText(/proves nothing about the current one/)).toBeTruthy();
  });

  it("shows no stale marking when the verdict names the assignment still in force", () => {
    render(<RunnerSandboxPanel runner={detail({ selftest: PASSING, selftest_completed_at: 1 })} />);
    expect(within(panel()).queryByText("stale")).toBeNull();
  });
});

describe("RunnerSandboxPanel — bind half", () => {
  it("states the baseline-only case rather than rendering an empty list", () => {
    render(<RunnerSandboxPanel runner={detail()} />);
    expect(within(panel()).getByText(/Baseline only/)).toBeTruthy();
  });

  it("reads binds as baseline-only when the runner carries no assignment at all", () => {
    render(<RunnerSandboxPanel runner={detail({ assigned_policy: null })} />);
    expect(within(panel()).getByText(/Baseline only/)).toBeTruthy();
  });

  it("lists each assigned path with its mode and the operator's note", () => {
    render(
      <RunnerSandboxPanel
        runner={detail({
          assigned_policy: {
            ...ASSIGNED,
            extra_binds: [{ path: "/srv/models", mode: "read_only", note: "gpu weights" }],
          },
        })}
      />,
    );
    expect(within(panel()).getByText("/srv/models")).toBeTruthy();
    expect(within(panel()).getByText("read-only")).toBeTruthy();
    expect(within(panel()).getByText("gpu weights")).toBeTruthy();
  });

  it("defaults an entry that names no mode to read-only", () => {
    render(
      <RunnerSandboxPanel
        runner={detail({ assigned_policy: { ...ASSIGNED, extra_binds: [{ path: "/srv/models" }] } })}
      />,
    );
    expect(within(panel()).getByText("read-only")).toBeTruthy();
  });

  it("marks a writable mount — it widens the isolation boundary for every lease", () => {
    render(
      <RunnerSandboxPanel
        runner={detail({
          assigned_policy: { ...ASSIGNED, extra_binds: [{ path: "/srv/cache", mode: "read_write" }] },
        })}
      />,
    );
    const badge = within(panel()).getByText("read-write");
    expect(badge.className).toContain("warning");
  });
});
