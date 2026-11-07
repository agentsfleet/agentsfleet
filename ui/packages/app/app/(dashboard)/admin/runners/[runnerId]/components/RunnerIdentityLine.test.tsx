import type { RunnerDetail } from "@/lib/api/runners";
import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { RunnerIdentityLine } from "./RunnerIdentityLine";

afterEach(cleanup);

it.each([true, false])("shows the assigned durable task state: %s", enabled => {
  const runner: RunnerDetail = {
    id: "01J2WQ8F3K7VZ9XB4N6MTYD5AR",
    host_id: "runner-prod-ams-01.internal",
    sandbox_tier: "landlock_full",
    status: "active",
    liveness: "busy",
    labels: ["gpu", "prod"],
    last_seen_at: new Date().toISOString(),
    created_at: new Date().toISOString(),
    assigned_policy: { durable_tasks_enabled: enabled,sandbox_tier: "landlock_full", network_policy: "allow_all", registry_allowlist: [], worker_count: 1, extra_binds: []},
    achievable: null,
    degraded: false,
    degraded_reason: null,
    selftest_requested_at: null,
    selftest_completed_at: null,
    selftest: null,
    active_lease_count: 2,
    active_fleet_count: 2,
    leases_acquired: 4021,
    leases_succeeded: 3945,
    leases_failed: 42,
    leases_expired: 34,
  };
  render(<RunnerIdentityLine runner={runner} adminState="active" />);
  expect(screen.getByText(`Durable tasks: ${enabled ? "On" : "Off"}`)).toBeTruthy();
});
