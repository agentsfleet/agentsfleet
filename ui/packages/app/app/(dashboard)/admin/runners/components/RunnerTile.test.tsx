import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import type { RunnerListItem } from "@/lib/api/runners";

const listRunnerLeasesActionMock = vi.fn();
vi.mock("../actions", () => ({
  listRunnerLeasesAction: (...args: unknown[]) => listRunnerLeasesActionMock(...args),
}));

import RunnerTile from "./RunnerTile";

afterEach(() => cleanup());
beforeEach(() => {
  listRunnerLeasesActionMock.mockReset();
});

function runner(overrides: Partial<RunnerListItem> = {}): RunnerListItem {
  return {
    id: "r-tile-1",
    host_id: "runner-prod-ams-01.internal",
    sandbox_tier: "landlock_full",
    admin_state: "active",
    liveness: "online",
    labels: ["gpu"],
    last_seen_at: Date.now(),
    created_at: Date.now(),
    ...overrides,
  };
}

function lease(fleetName: string) {
  return {
    id: `lease-${fleetName}`,
    fleet_id: `fleet-${fleetName}`,
    fleet_name: fleetName,
    workspace_id: "ws-1",
    event_id: "evt-1",
    event_type: "chat",
    actor: "system",
    outcome: "running",
    failure_label: null,
    failure_detail: null,
    kind: "fresh",
    fencing_token: 1,
    provider: "anthropic",
    model: "claude",
    posture: "metered",
    metered_input_tokens: 0,
    metered_cached_tokens: 0,
    metered_output_tokens: 0,
    wall_ms: null,
    lease_expires_at: Date.now() + 60_000,
    created_at: Date.now(),
  };
}

describe("RunnerTile", () => {
  it("test_runner_wall_card_links_to_detail", () => {
    render(<RunnerTile runner={runner()} />);
    const link = screen.getByRole("link");
    expect(link.getAttribute("href")).toBe("/admin/runners/r-tile-1");
  });

  it("test_runner_tile_states_current_work_or_idle", async () => {
    // Busy: the fleets are named from the live leases.
    listRunnerLeasesActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [lease("Production API"), lease("Billing Workers")],
        total: 2,
        next_cursor: null,
      },
    });
    render(<RunnerTile runner={runner({ liveness: "busy" })} />);
    await waitFor(() => {
      expect(screen.getByText("2 leases · Production API, Billing Workers")).toBeTruthy();
    });

    // Idle: the idle sentence renders, and nothing is fetched.
    cleanup();
    render(<RunnerTile runner={runner({ liveness: "online" })} />);
    expect(screen.getByText("Idle. No active leases.")).toBeTruthy();
    expect(listRunnerLeasesActionMock).toHaveBeenCalledTimes(1);
  });
});
