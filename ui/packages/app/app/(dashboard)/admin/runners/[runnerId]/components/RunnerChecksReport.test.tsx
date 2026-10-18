import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { RunnerDetail, SelftestReport } from "@/lib/api/runners";
import type { AssignedPolicy } from "@/lib/api/runners-types";
import { RunnerChecksReport } from "./RunnerChecksReport";

afterEach(() => cleanup());

const renderPanel = (ui: ReactElement) => render(ui, { wrapper: TooltipProvider });
const NO_ADDITIONAL_MOUNTS = "No additional mounts.";

const ASSIGNED: AssignedPolicy = {
  sandbox_tier: "landlock_full",
  network_policy: "deny_all_egress",
  registry_allowlist: [],
  worker_count: 2,
  extra_binds: [],
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
    status: "active",
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
  return screen.getByTestId("runner-checks-report");
}

describe("RunnerChecksReport — self-test half", () => {
  it("says the runner has never been tested rather than showing an empty verdict", () => {
    renderPanel(<RunnerChecksReport runner={detail()} />);
    expect(within(panel()).getByText(/Never checked/)).toBeTruthy();
    expect(screen.queryByText("all checks passed")).toBeNull();
  });


  it("names an outstanding request so a blank verdict does not read as a healthy one", () => {
    renderPanel(<RunnerChecksReport runner={detail({ selftest_requested_at: 1_760_000_000_000 })} />);
    expect(within(panel()).getByText(/Checks are outstanding/)).toBeTruthy();
  });

  // Dimension 1.2 — each check the host reported gets its own name and detail
  // line, so "DNS failed inside the sandbox" reads without a journal.
  it("test_selftest_result_renders_per_check", () => {
    renderPanel(
      <RunnerChecksReport
        runner={detail({ selftest: PASSING, selftest_completed_at: 1_760_000_000_000 })}
      />,
    );
    expect(within(panel()).getByText("all checks passed")).toBeTruthy();
    expect(within(panel()).getByText("resolver file resolves inside the sandbox")).toBeTruthy();
    expect(within(panel()).queryByText("no fault detected")).toBeNull();
    expect(within(panel()).queryByRole("heading", { name: "Checks" })).toBeNull();
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
    renderPanel(<RunnerChecksReport runner={detail({ selftest: failing, selftest_completed_at: 1 })} />);
    expect(within(panel()).getByText("2 failed")).toBeTruthy();
    expect(within(panel()).getByText("the stub is not bound")).toBeTruthy();
  });

  it("keeps the report free of relative timestamps", () => {
    renderPanel(<RunnerChecksReport runner={detail({ selftest: PASSING, selftest_completed_at: 1_760_000_000_000 })} />);
    expect(panel().querySelector("time")).toBeNull();
  });

  // Dimension 1.3 — the result is history, and the page has to say so or an
  // operator reads a passing verdict as proof of the current policy.
  it("test_stale_selftest_result_is_labelled", () => {
    renderPanel(
      <RunnerChecksReport
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
    renderPanel(<RunnerChecksReport runner={detail({ selftest: PASSING, selftest_completed_at: 1 })} />);
    expect(within(panel()).queryByText("stale")).toBeNull();
  });
});

describe("RunnerChecksReport — bind half", () => {
  it("states when no additional mounts are assigned", () => {
    renderPanel(<RunnerChecksReport runner={detail()} />);
    expect(within(panel()).getByText(NO_ADDITIONAL_MOUNTS)).toBeTruthy();
  });

  it("lists each assigned path with its mode and the operator's note", () => {
    renderPanel(
      <RunnerChecksReport
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

  it("labels a read-only mount and shows no note when the note is blank", () => {
    renderPanel(
      <RunnerChecksReport
        runner={detail({ assigned_policy: { ...ASSIGNED, extra_binds: [{ path: "/srv/models", mode: "read_only", note: "" }] } })}
      />,
    );
    expect(within(panel()).getByText("read-only")).toBeTruthy();
    expect(within(panel()).getByText("/srv/models").parentElement?.textContent).toBe("/srv/modelsread-only");
  });

  it("marks a writable mount — it widens the isolation boundary for every lease", () => {
    renderPanel(
      <RunnerChecksReport
        runner={detail({
          assigned_policy: { ...ASSIGNED, extra_binds: [{ path: "/srv/cache", mode: "read_write", note: "" }] },
        })}
      />,
    );
    const badge = within(panel()).getByText("read-write");
    expect(badge.className).toContain("warning");
  });
});
