import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { RunnerDetail } from "@/lib/api/runners";
import {
  DURABLE_TASKS_OFF_LABEL,
  DURABLE_TASKS_ON_LABEL,
  RunnerIdentityLine,
} from "./RunnerIdentityLine";

afterEach(() => cleanup());

function runner(durable: boolean): RunnerDetail {
  return {
    id: "r-identity-1",
    host_id: "runner-prod-ams-01.internal",
    sandbox_tier: "landlock_full",
    status: "active",
    liveness: "online",
    labels: [],
    last_seen_at: new Date(Date.now() - 8_000).toISOString(),
    created_at: new Date(Date.now() - 86_400_000).toISOString(),
    assigned_policy: {
      durable_tasks_enabled: durable,
      sandbox_tier: "landlock_full",
      network_policy: "allow_all",
      registry_allowlist: [],
      worker_count: 1,
      extra_binds: [],
    },
    achievable: null,
    degraded: false,
    selftest_requested_at: null,
    selftest_completed_at: null,
    selftest: null,
    degraded_reason: null,
    active_lease_count: 0,
    active_fleet_count: 0,
    leases_acquired: 0,
    leases_succeeded: 0,
    leases_failed: 0,
    leases_expired: 0,
  };
}

describe("RunnerIdentityLine", () => {
  it("names whether the runner takes durable-task work", () => {
    render(<RunnerIdentityLine runner={runner(true)} adminState="active" />);
    expect(screen.getByTestId("runner-durable-tasks").textContent).toBe(DURABLE_TASKS_ON_LABEL);
    cleanup();
    render(<RunnerIdentityLine runner={runner(false)} adminState="active" />);
    expect(screen.getByTestId("runner-durable-tasks").textContent).toBe(DURABLE_TASKS_OFF_LABEL);
  });
});
