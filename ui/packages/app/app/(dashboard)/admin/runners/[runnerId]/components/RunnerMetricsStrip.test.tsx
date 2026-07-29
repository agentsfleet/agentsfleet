import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { RunnerDetail } from "@/lib/api/runners";
import RunnerMetricsStrip from "./RunnerMetricsStrip";

afterEach(() => cleanup());

const RUNNER: RunnerDetail = {
  id: "r-strip-1",
  host_id: "runner-prod-ams-01.internal",
  sandbox_tier: "landlock_full",
  admin_state: "active",
  liveness: "busy",
  labels: [],
  last_seen_at: Date.now() - 8_000,
  created_at: Date.now() - 86_400_000,
  active_lease_count: 2,
  active_fleet_count: 2,
  leases_acquired: 4021,
  leases_succeeded: 3945,
  leases_failed: 42,
  leases_expired: 34,
};

describe("RunnerMetricsStrip", () => {
  it("test_runner_metrics_strip_cells_and_colours", () => {
    render(<RunnerMetricsStrip runner={RUNNER} />);
    // Six cells, by their uppercase labels.
    for (const label of ["Heartbeat", "Leases now", "Acquired", "Succeeded", "Failed", "Expired"]) {
      expect(screen.getByText(label)).toBeTruthy();
    }
    // Outcome counters carry distinct status colours — the same tokens the
    // row badges use — while the neutral counters do not.
    expect(screen.getByText("3,945").className).toContain("text-success");
    expect(screen.getByText("42").className).toContain("text-error");
    expect(screen.getByText("34").className).toContain("text-warn");
    expect(screen.getByText("4,021").className).not.toContain("text-success");
    // The live-work detail names the fleet spread; the expired detail keeps
    // the reviewed phrasing.
    expect(screen.getByText("across 2 Fleets")).toBeTruthy();
    expect(screen.getByText("not renewed")).toBeTruthy();
  });
});
