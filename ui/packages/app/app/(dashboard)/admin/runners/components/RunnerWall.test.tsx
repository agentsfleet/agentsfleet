import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { RunnerListItem } from "@/lib/api/runners";

const listRunnersActionMock = vi.fn();
vi.mock("../actions", () => ({
  listRunnersAction: (...args: unknown[]) => listRunnersActionMock(...args),
  listRunnerLeasesAction: vi.fn().mockResolvedValue({ ok: true, data: { items: [], total: 0, next_cursor: null } }),
}));

import RunnerWall from "./RunnerWall";

afterEach(() => cleanup());
beforeEach(() => {
  listRunnersActionMock.mockReset();
});

describe("RunnerWall", () => {
  it("test_runner_wall_empty_state", () => {
    render(<RunnerWall initialRunners={[]} initialCursor={null} />);
    expect(screen.getByText("No runners enrolled")).toBeTruthy();
    expect(screen.queryAllByRole("link")).toHaveLength(0);
  });

  it("renders one card per runner", () => {
    render(
      <RunnerWall
        initialRunners={[
          {
            id: "wall-a",
            host_id: "host-a",
            sandbox_tier: "landlock_full",
            admin_state: "active",
            liveness: "online",
            labels: [],
            last_seen_at: Date.now(),
            created_at: 2,
            assigned_policy: null,
            achievable: null,
            degraded: false,
            degraded_reason: null,
          },
          {
            id: "wall-b",
            host_id: "host-b",
            sandbox_tier: "dev_none",
            admin_state: "cordoned",
            liveness: "offline",
            labels: [],
            last_seen_at: 0,
            created_at: 1,
            assigned_policy: null,
            achievable: null,
            degraded: false,
            degraded_reason: null,
          },
        ]}
        initialCursor={null}
      />,
    );
    const links = screen.getAllByRole("link");
    expect(links).toHaveLength(2);
    expect(links.map((l) => l.getAttribute("href"))).toEqual([
      "/admin/runners/wall-a",
      "/admin/runners/wall-b",
    ]);
  });

  const seed: RunnerListItem = {
    id: "wall-seed",
    host_id: "host-seed",
    sandbox_tier: "landlock_full",
    admin_state: "active",
    liveness: "online",
    labels: [],
    last_seen_at: Date.now(),
    created_at: 3,
    assigned_policy: null,
    achievable: null,
    degraded: false,
    degraded_reason: null,
  };

  it("should append the next page and follow its cursor when Load more succeeds", async () => {
    listRunnersActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [{ ...seed, id: "wall-next", host_id: "host-next" }],
        total: 2,
        next_cursor: null,
      },
    });
    render(<RunnerWall initialRunners={[seed]} initialCursor={"wall-seed"} />);
    fireEvent.click(screen.getByRole("button", { name: "Load more" }));
    await waitFor(() => {
      expect(screen.getAllByRole("link")).toHaveLength(2);
    });
    expect(listRunnersActionMock).toHaveBeenCalledWith({ starting_after: "wall-seed" });
    // The server said the collection ended, so the control leaves with it.
    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
  });

  it("should surface a Load more failure without dropping the cards already shown", async () => {
    listRunnersActionMock.mockResolvedValueOnce({
      ok: false,
      errorCode: "UZ-REQ-001",
      error: "starting_after did not parse",
    });
    render(<RunnerWall initialRunners={[seed]} initialCursor={"wall-seed"} />);
    fireEvent.click(screen.getByRole("button", { name: "Load more" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toBeTruthy();
    // The wall keeps what it had, and the control survives for a retry (it
    // reads "Loading…" until the async transition settles, so wait for it).
    expect(screen.getAllByRole("link")).toHaveLength(1);
    expect(await screen.findByRole("button", { name: "Load more" })).toBeTruthy();
  });
});
