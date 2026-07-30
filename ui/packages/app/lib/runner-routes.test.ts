import { describe, expect, it } from "vitest";
import { resolveRunnerView, runnerPath, runnersIndexPath, RUNNER_VIEW } from "./runner-routes";

describe("runner routes", () => {
  it("test_runner_view_resolves_to_leases_by_default", () => {
    // Absent, empty and unknown all land on the page's main object.
    expect(resolveRunnerView(undefined)).toBe(RUNNER_VIEW.leases);
    expect(resolveRunnerView("")).toBe(RUNNER_VIEW.leases);
    expect(resolveRunnerView("overview")).toBe(RUNNER_VIEW.leases);
    expect(resolveRunnerView(RUNNER_VIEW.leases)).toBe(RUNNER_VIEW.leases);
    expect(resolveRunnerView(RUNNER_VIEW.activity)).toBe(RUNNER_VIEW.activity);
  });

  it("writes the route string once, defaulting the view out of the address", () => {
    expect(runnersIndexPath()).toBe("/admin/runners");
    expect(runnerPath("r1")).toBe("/admin/runners/r1");
    expect(runnerPath("r1", RUNNER_VIEW.leases)).toBe("/admin/runners/r1");
    expect(runnerPath("r1", RUNNER_VIEW.activity)).toBe("/admin/runners/r1?view=activity");
  });
});
