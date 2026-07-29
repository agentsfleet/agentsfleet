import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("../actions", () => ({
  listRunnersAction: vi.fn(),
  listRunnerLeasesAction: vi.fn().mockResolvedValue({ ok: true, data: { items: [], total: 0, next_cursor: null } }),
}));

import RunnerWall from "./RunnerWall";

afterEach(() => cleanup());

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
});
