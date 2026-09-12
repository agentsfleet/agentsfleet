import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { RunnerDetail } from "@/lib/api/runners";
import RunnerStatusLine from "./RunnerStatusLine";

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
  assigned_policy: null,
  achievable: null,
  degraded: false,
  selftest_requested_at: null,
  selftest_completed_at: null,
  selftest: null,
  degraded_reason: null,
  active_lease_count: 2,
  active_fleet_count: 2,
  leases_acquired: 4021,
  leases_succeeded: 3945,
  leases_failed: 42,
  leases_expired: 34,
};

const line = () => screen.getByLabelText("Runner metrics");
const toneOf = (text: string | RegExp) => screen.getByText(text).closest("[data-tone]")?.getAttribute("data-tone");

describe("RunnerStatusLine", () => {
  it("test_runner_status_line_cells_and_colours", () => {
    render(<RunnerStatusLine runner={RUNNER} />);
    expect(line().className).toContain("font-mono");
    // Three cells: a beating heart, the outcome pair, the live count.
    expect(screen.getByText(/seconds ago/)).toBeTruthy();
    expect(toneOf(/seconds ago/)).toBe("pulse");
    // Outcome counters carry distinct status colours — the same tokens the
    // row badges use.
    expect(screen.getByText("3,945 ok").className).toContain("text-success");
    expect(screen.getByText("42 failed").className).toContain("text-error");
    expect(screen.getByText("2 live")).toBeTruthy();
    expect(toneOf("2 live")).toBe("foreground");
    // The lifetime ledger stays off the line.
    expect(screen.queryByText("4,021")).toBeNull();
    expect(screen.queryByText("34")).toBeNull();
  });

  it("keeps every cell's label for screen readers", () => {
    render(<RunnerStatusLine runner={RUNNER} />);
    for (const label of ["Heartbeat", "Lease outcomes", "Leases now"]) {
      expect(screen.getByText(label).className).toContain("sr-only");
    }
  });

  it("should dash the heartbeat for a never-seen runner and quiet an idle one", () => {
    render(<RunnerStatusLine runner={{ ...RUNNER, last_seen_at: 0, active_lease_count: 0 }} />);
    // last_seen_at = 0 is the never-connected sentinel: an honest dash, no
    // fabricated relative time.
    expect(screen.getByText("—")).toBeTruthy();
    expect(toneOf("—")).toBe("neutral");
    expect(line().querySelector("time")).toBeNull();
    expect(toneOf("0 live")).toBe("neutral");
  });

  it("merges the caller's placement classes", () => {
    render(<RunnerStatusLine runner={RUNNER} className="sticky bottom-0" />);
    expect(line().className).toContain("sticky");
    expect(line().className).toContain("border-t");
  });
});
