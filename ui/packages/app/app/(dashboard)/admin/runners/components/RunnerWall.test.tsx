import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
            status: "active",
            liveness: "online",
            labels: [],
            last_seen_at: Date.now(),
            created_at: 2,
            assigned_policy: {sandbox_tier: "landlock_full", network_policy: "allow_all", registry_allowlist: [], worker_count: 1, extra_binds: []},
            achievable: null,
            degraded: false,
            degraded_reason: null,
          },
          {
            id: "wall-b",
            host_id: "host-b",
            sandbox_tier: "dev_none",
            status: "cordoned",
            liveness: "offline",
            labels: [],
            last_seen_at: 0,
            created_at: 1,
            assigned_policy: {sandbox_tier: "dev_none", network_policy: "allow_all", registry_allowlist: [], worker_count: 1, extra_binds: []},
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
    status: "active",
    liveness: "online",
    labels: [],
    last_seen_at: Date.now(),
    created_at: 3,
    assigned_policy: {sandbox_tier: "landlock_full", network_policy: "allow_all", registry_allowlist: [], worker_count: 1, extra_binds: []},
    achievable: null,
    degraded: false,
    degraded_reason: null,
  };

  it("shows the next page and returns to cached rows without a request", async () => {
    listRunnersActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [{ ...seed, id: "wall-next", host_id: "host-next" }],
        total: 2,
        next_cursor: null,
      },
    });
    render(<RunnerWall initialRunners={[seed]} initialCursor={"wall-seed"} />);
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(await screen.findByText("host-next")).toBeTruthy();
    expect(listRunnersActionMock).toHaveBeenCalledWith({ starting_after: "wall-seed", limit: 25 });
    // The server said the collection ended, so the control leaves with it.
    expect(screen.getByRole("button", { name: "Next page" })).toHaveProperty("disabled", true);
    expect(screen.queryByText("host-seed")).toBeNull();
    await waitFor(() => expect(screen.getByRole("button", {name: "Previous page"})).toHaveProperty("disabled", false));
    fireEvent.click(screen.getByRole("button", {name: "Previous page"}));
    expect(screen.getByText("host-seed")).toBeTruthy();
    expect(listRunnersActionMock).toHaveBeenCalledOnce();
  });

  it("reconciles a refreshed list and ignores an older outstanding page", async () => {
    let finish!: (value: unknown) => void;
    listRunnersActionMock.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const view = render(<RunnerWall initialRunners={[seed]} initialCursor="older" />);
    fireEvent.click(screen.getByRole("button", {name: "Next page"}));
    view.rerender(<RunnerWall initialRunners={[{...seed, host_id: "refreshed-host"}]} initialCursor={null} />);
    expect(screen.getByText("refreshed-host")).toBeTruthy();
    finish({ok: true, data: {items: [{...seed, host_id: "stale-host"}], next_cursor: null}});
    await waitFor(() => expect(screen.queryByRole("button", {name: "Next page"})).toBeNull());
    expect(screen.getByText("refreshed-host")).toBeTruthy();
    expect(screen.queryByText("stale-host")).toBeNull();
  });

  it("keeps its current rows and offers retry when the request itself rejects", async () => {
    listRunnersActionMock.mockRejectedValue(new Error("private-response-marker"));
    render(<RunnerWall initialRunners={[seed]} initialCursor="older" />);
    fireEvent.click(screen.getByRole("button", {name: "Next page"}));
    expect((await screen.findByRole("alert")).textContent).not.toContain("private-response-marker");
    expect(screen.getByText("host-seed")).toBeTruthy();
    await waitFor(() => expect(screen.getByRole("button", {name: "Next page"})).toHaveProperty("disabled", false));
  });

  it("should surface a Load more failure without dropping the cards already shown", async () => {
    listRunnersActionMock.mockResolvedValueOnce({
      ok: false,
      errorCode: "UZ-REQ-001",
      error: "starting_after did not parse",
    });
    render(<RunnerWall initialRunners={[seed]} initialCursor={"wall-seed"} />);
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toBeTruthy();
    // The wall keeps what it had, and the control survives for a retry (it
    // reads "Loading…" until the async transition settles, so wait for it).
    expect(screen.getAllByRole("link")).toHaveLength(1);
    expect(await screen.findByRole("button", { name: "Next page" })).toBeTruthy();
  });
  it("keeps the refreshed collection when an old page rejects", async () => {
    const older = Promise.withResolvers<unknown>();
    listRunnersActionMock.mockReturnValueOnce(older.promise);
    const view = render(<RunnerWall initialRunners={[seed]} initialCursor="older" />);
    fireEvent.click(screen.getByRole("button", {name: "Next page"}));
    view.rerender(<RunnerWall initialRunners={[{...seed, host_id: "current-host"}]} initialCursor={null} />);
    await act(async () => {older.reject(new Error("old page failed"));});
    expect(screen.getByText("current-host")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("stops paging when the server repeats its cursor", async () => {
    listRunnersActionMock.mockResolvedValueOnce({ok: true, data: {items: [seed], next_cursor: "same"}});
    render(<RunnerWall initialRunners={[seed]} initialCursor="same" />);
    fireEvent.click(screen.getByRole("button", {name: "Next page"}));
    await waitFor(() => expect(screen.getByText("Page 2")).toBeTruthy());
    expect(screen.getByRole("button", {name: "Next page"})).toHaveProperty("disabled", true);
  });

  it.each(["success", "failure"])("ignores an old page %s when only the initial cursor changes", async (outcome) => {
    const rows = [seed];
    const older = Promise.withResolvers<unknown>();
    listRunnersActionMock.mockReturnValueOnce(older.promise)
      .mockResolvedValueOnce({ok: true, data: {items: [{...seed, host_id: "fresh-host"}], next_cursor: null}});
    const view = render(<RunnerWall initialRunners={rows} initialCursor="cursor-a" />);
    fireEvent.click(screen.getByRole("button", {name: "Next page"}));
    view.rerender(<RunnerWall initialRunners={rows} initialCursor="cursor-b" />);
    await act(async () => {
      if (outcome === "success") older.resolve({ok: true, data: {items: [{...seed, host_id: "stale-host"}], next_cursor: null}});
      else older.reject(new Error("obsolete page failed"));
    });
    expect(screen.getByText(seed.host_id)).toBeTruthy();
    expect(screen.queryByText("stale-host")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("Page 1")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", {name: "Next page"}));
    expect(await screen.findByText("fresh-host")).toBeTruthy();
    expect(listRunnersActionMock).toHaveBeenLastCalledWith({starting_after: "cursor-b", limit: 25});
  });

  it("walks cached pages in either direction without rereading them", async () => {
    listRunnersActionMock.mockResolvedValueOnce({ok: true, data: {items: [{...seed, host_id: "second-host"}], next_cursor: "third"}})
      .mockResolvedValueOnce({ok: true, data: {items: [{...seed, host_id: "third-host"}], next_cursor: null}});
    render(<RunnerWall initialRunners={[seed]} initialCursor="second" />);
    for (const host of ["second-host", "third-host"]) {
      await waitFor(() => expect(screen.getByRole("button", {name: "Next page"})).toHaveProperty("disabled", false));
      fireEvent.click(screen.getByRole("button", {name: "Next page"}));
      await screen.findByText(host);
    }
    await waitFor(() => expect(screen.getByRole("button", {name: "Previous page"})).toHaveProperty("disabled", false));
    fireEvent.click(screen.getByRole("button", {name: "Previous page"}));
    expect(screen.getByText("second-host")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", {name: "Next page"}));
    expect(screen.getByText("third-host")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", {name: "Previous page"}));
    fireEvent.click(screen.getByRole("button", {name: "Previous page"}));
    expect(screen.getByText("host-seed")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", {name: "Next page"}));
    expect(screen.getByText("second-host")).toBeTruthy();
    expect(listRunnersActionMock).toHaveBeenCalledTimes(2);
  });

});
