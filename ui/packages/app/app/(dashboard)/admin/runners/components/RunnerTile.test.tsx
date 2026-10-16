import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
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
    assigned_policy: {sandbox_tier: "landlock_full", network_policy: "allow_all", registry_allowlist: [], worker_count: 1, extra_binds: []},
    achievable: null,
    degraded: false,
    degraded_reason: null,
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
  it("refreshes busy activity when a new server snapshot arrives, without repeating local rerenders", async () => {
    listRunnerLeasesActionMock.mockResolvedValueOnce({ok: true, data: {items: [lease("Build fleet")], next_cursor: null}})
      .mockResolvedValueOnce({ok: true, data: {items: [lease("Deploy fleet")], next_cursor: null}});
    const row = runner({liveness: "busy"});
    const view = render(<RunnerTile runner={row} />);
    await screen.findByText("1 lease · Build fleet");
    view.rerender(<RunnerTile runner={row} />);
    expect(listRunnerLeasesActionMock).toHaveBeenCalledOnce();
    view.rerender(<RunnerTile runner={{...row}} />);
    expect(await screen.findByText("1 lease · Deploy fleet")).toBeTruthy();
    expect(listRunnerLeasesActionMock).toHaveBeenCalledTimes(2);
  });
  it("contains a rejected activity read and keeps the runner inspectable", async () => {
    listRunnerLeasesActionMock.mockRejectedValueOnce(new Error("private-lease-error"))
      .mockResolvedValueOnce({ok: true, data: {items: [lease("Recovered fleet")], next_cursor: null}});
    const row = runner({liveness: "busy"});
    const view = render(<RunnerTile runner={row} />);
    await screen.findByText("Activity unavailable. Open runner to retry.");
    expect(screen.getByRole("link").getAttribute("href")).toBe("/admin/runners/r-tile-1");
    expect(screen.queryByText("private-lease-error")).toBeNull();
    view.rerender(<RunnerTile runner={{...row}} />);
    expect(await screen.findByText("1 lease · Recovered fleet")).toBeTruthy();
  });

  it("test_runner_wall_card_links_to_detail", () => {
    render(<RunnerTile runner={runner()} />);
    const link = screen.getByRole("link");
    expect(link.getAttribute("href")).toBe("/admin/runners/r-tile-1");
  });

  it("test_degraded_runner_row_states_the_reason", () => {
    // Dimensions 4.1/4.2 (tile face) — a degraded runner is visually distinct
    // and names the specific missing mechanism in place of the work line.
    render(
      <RunnerTile
        runner={runner({ degraded: true, degraded_reason: "landlock unavailable" })}
      />,
    );
    expect(screen.getByText("degraded")).toBeTruthy();
    expect(screen.getByText("landlock unavailable")).toBeTruthy();
  });

  it("a healthy runner shows no degraded badge", () => {
    render(<RunnerTile runner={runner()} />);
    expect(screen.queryByText("degraded")).toBeNull();
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

  it("reports unavailable activity when a busy runner lease read fails", async () => {
    listRunnerLeasesActionMock.mockResolvedValueOnce({
      ok: false,
      errorCode: "UZ-RUN-014",
      error: "runner read failed",
    });
    render(<RunnerTile runner={runner({ liveness: "busy" })} />);
    // Failed activity reads must not contradict the known busy status.
    await waitFor(() => {
      expect(screen.getByText("Activity unavailable. Open runner to retry.")).toBeTruthy();
      expect(screen.queryByText("Idle. No active leases.")).toBeNull();
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


it("keeps refreshed activity when the old request rejects afterwards", async () => {
  const older = Promise.withResolvers<unknown>();
  listRunnerLeasesActionMock.mockReturnValueOnce(older.promise).mockResolvedValueOnce({ok: true,
    data: {items: [lease("Current Fleet")], total: 1, next_cursor: null}});
  const view = render(<RunnerTile runner={runner({liveness: "busy"})} />);
  view.rerender(<RunnerTile runner={runner({liveness: "busy", last_seen_at: Date.now() + 1})} />);
  await screen.findByText("1 lease · Current Fleet");
  await act(async () => {older.reject(new Error("old request failed"));});
  expect(screen.getByText("1 lease · Current Fleet")).toBeTruthy();
  expect(screen.queryByText(/Activity unavailable/)).toBeNull();
});
