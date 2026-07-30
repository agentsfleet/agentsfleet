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

  it("should fall back to the idle sentence when the lease read fails", async () => {
    listRunnerLeasesActionMock.mockResolvedValueOnce({
      ok: false,
      errorCode: "UZ-RUN-014",
      error: "runner read failed",
    });
    render(<RunnerTile runner={runner({ liveness: "busy" })} />);
    // A failed work-line read degrades to idle copy — never an error state on
    // a card, and never a fabricated fleet name.
    await waitFor(() => {
      expect(screen.getByText("Idle. No active leases.")).toBeTruthy();
    });
  });

  it("should read idle when the lease page carries no running lease", async () => {
    listRunnerLeasesActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [{ ...lease("Settled"), outcome: "succeeded" }],
        total: 1,
        next_cursor: null,
      },
    });
    render(<RunnerTile runner={runner({ liveness: "busy" })} />);
    await waitFor(() => {
      expect(screen.getByText("Idle. No active leases.")).toBeTruthy();
    });
  });

  it("should speak in the singular for one running lease and fall back to the fleet id", async () => {
    listRunnerLeasesActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [{ ...lease("solo"), fleet_name: null, fleet_id: "fleet-solo-id" }],
        total: 1,
        next_cursor: null,
      },
    });
    render(<RunnerTile runner={runner({ liveness: "busy" })} />);
    await waitFor(() => {
      expect(screen.getByText("1 lease · fleet-solo-id")).toBeTruthy();
    });
  });

  it("should read never-connected for a freshly minted runner", () => {
    render(<RunnerTile runner={runner({ liveness: "registered", last_seen_at: 0 })} />);
    // The sentence appears in both the work line and the last-seen line —
    // present is what matters, and no lease read ever fires for it.
    expect(screen.getAllByText("Never connected.").length).toBeGreaterThan(0);
    expect(listRunnerLeasesActionMock).not.toHaveBeenCalled();
  });

  it("should ignore a stale work-line answer when the tile switches runners mid-flight", async () => {
    let resolveFirst: (value: unknown) => void = () => {};
    listRunnerLeasesActionMock
      .mockReturnValueOnce(
        new Promise((resolve) => {
          resolveFirst = resolve;
        }),
      )
      .mockResolvedValueOnce({
        ok: true,
        data: { items: [lease("Second Fleet")], total: 1, next_cursor: null },
      });

    const { rerender } = render(<RunnerTile runner={runner({ id: "first", liveness: "busy" })} />);
    // Switch runners while the first read is still in flight — the effect's
    // cleanup marks that first answer stale.
    rerender(<RunnerTile runner={runner({ id: "second", liveness: "busy" })} />);
    await waitFor(() => {
      expect(screen.getByText("1 lease · Second Fleet")).toBeTruthy();
    });

    // The stale answer lands last. Without the cancelled guard it would
    // overwrite the line the current runner already resolved.
    resolveFirst({
      ok: true,
      data: { items: [lease("Stale Fleet")], total: 1, next_cursor: null },
    });
    await waitFor(() => {
      expect(screen.getByText("1 lease · Second Fleet")).toBeTruthy();
    });
    expect(screen.queryByText(/Stale Fleet/)).toBeNull();
  });
});
