import { EMPTY_PAGE, RUNNER, getRunnerMock, listRunnerEventsMock, listRunnerLeasesMock, loadPage, mockAuth, pageProps } from "./harness";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

describe("admin/runners/[runnerId] page — views and filters", () => {
  it("lands on Leases by default: the table, the status line at the foot, tracker armed, no Grafana without a base", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockResolvedValueOnce({
      items: [{ id: "lease-1" }],
      total: 1,
      next_cursor: null,
    });
    const Page = await loadPage();
    // No searchParams at all — the arm a bare route navigation takes.
    const html = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ runnerId: RUNNER.id }) }),
    );
    expect(html).toContain('data-runner-header="runner-prod-ams-01.internal"');
    expect(html).toContain('data-grafana="none"');
    expect(html).toContain('data-runner-rail="leases"');
    expect(html).toContain('data-runner-status-line="1"');
    expect(html).toContain('data-lease-table="1"');
    expect(html).toContain('data-runner-viewed="active:busy"');
    expect(listRunnerLeasesMock).toHaveBeenCalledWith("tok", RUNNER.id, { limit: 25 });
    expect(listRunnerEventsMock).not.toHaveBeenCalled();
  });

  it("serves Activity with the lifecycle type set and the same status line at the foot", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerEventsMock.mockResolvedValueOnce({
      items: [{ id: "evt-1" }, { id: "evt-2" }],
      total: 2,
      next_cursor: null,
    });
    const Page = await loadPage();
    const html = renderToStaticMarkup(await Page(pageProps({ view: "activity" })));
    expect(html).toContain('data-runner-rail="activity"');
    expect(html).toContain('data-activity-table="2"');
    expect(html).toContain('data-runner-status-line="1"');
    expect(listRunnerEventsMock).toHaveBeenCalledWith(
      "tok",
      RUNNER.id,
      expect.objectContaining({
        event_type:
          "runner_registered,runner_online,runner_offline,runner_cordoned,runner_draining,runner_drained,runner_revoked,runner_policy_assigned",
      }),
    );
    expect(listRunnerLeasesMock).not.toHaveBeenCalled();
  });

  it("forwards the cursor trail as starting_after on both views", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockResolvedValueOnce(EMPTY_PAGE);
    const Page = await loadPage();
    renderToStaticMarkup(await Page(pageProps({ c: "lease-cursor-1", cps: "25" })));
    expect(listRunnerLeasesMock).toHaveBeenCalledWith("tok", RUNNER.id, {
      limit: 25,
      starting_after: "lease-cursor-1",
    });

    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerEventsMock.mockResolvedValueOnce(EMPTY_PAGE);
    renderToStaticMarkup(
      await Page(pageProps({ view: "activity", c: "evt-cursor-1", cps: "25" })),
    );
    expect(listRunnerEventsMock).toHaveBeenCalledWith(
      "tok",
      RUNNER.id,
      expect.objectContaining({ starting_after: "evt-cursor-1" }),
    );
  });

  it("narrows the lease read to the workspace the URL names", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockResolvedValueOnce(EMPTY_PAGE);
    const Page = await loadPage();
    renderToStaticMarkup(await Page(pageProps({ workspace: "ws-0123456789" })));
    expect(listRunnerLeasesMock).toHaveBeenCalledWith("tok", RUNNER.id, {
      limit: 25,
      workspace_id: "ws-0123456789",
    });
  });

  it("composes the workspace filter with the cursor trail rather than replacing it", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockResolvedValueOnce(EMPTY_PAGE);
    const Page = await loadPage();
    renderToStaticMarkup(
      await Page(pageProps({ workspace: "ws-0123456789", c: "lease-cursor-1", cps: "25" })),
    );
    expect(listRunnerLeasesMock).toHaveBeenCalledWith("tok", RUNNER.id, {
      limit: 25,
      starting_after: "lease-cursor-1",
      workspace_id: "ws-0123456789",
    });
  });

  it.each([
    ["empty", ""],
    ["repeated", ["ws-a", "ws-b"]],
  ])(
    "fails closed to the unfiltered feed for a %s workspace param",
    async (_name, value) => {
      mockAuth();
      getRunnerMock.mockResolvedValueOnce(RUNNER);
      listRunnerLeasesMock.mockResolvedValueOnce(EMPTY_PAGE);
      const Page = await loadPage();
      renderToStaticMarkup(await Page(pageProps({ workspace: value })));
      // No `workspace_id` key at all — an unfiltered read. Sending an empty or
      // ambiguous value onward would earn a 400 from the daemon's validator and
      // render the error state for what is really just a malformed link.
      expect(listRunnerLeasesMock).toHaveBeenCalledWith("tok", RUNNER.id, { limit: 25 });
    },
  );

  it("narrows the lease read to the fleet the URL names", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockResolvedValueOnce(EMPTY_PAGE);
    const Page = await loadPage();
    renderToStaticMarkup(await Page(pageProps({ fleet: "billing-reconciler" })));
    // A NAME, passed through untouched. The server matches `fleet` against an id
    // OR an exact name, so resolving it to an id here would break filtering by
    // the thing the table actually shows the operator.
    expect(listRunnerLeasesMock).toHaveBeenCalledWith("tok", RUNNER.id, {
      limit: 25,
      fleet: "billing-reconciler",
    });
  });

  it("sends both filters when the URL names a workspace and a fleet, so they intersect", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockResolvedValueOnce(EMPTY_PAGE);
    const Page = await loadPage();
    renderToStaticMarkup(
      await Page(pageProps({ workspace: "ws-0123456789", fleet: "billing-reconciler" })),
    );
    // Both keys, not one. Dropping either here would silently widen the feed to
    // a set the operator did not ask for, and the intersect rule the server
    // enforces would never be reached.
    expect(listRunnerLeasesMock).toHaveBeenCalledWith("tok", RUNNER.id, {
      limit: 25,
      workspace_id: "ws-0123456789",
      fleet: "billing-reconciler",
    });
  });

  it.each([
    ["empty", ""],
    ["repeated", ["fleet-a", "fleet-b"]],
  ])("fails closed to the unfiltered feed for a %s fleet param", async (_name, value) => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockResolvedValueOnce(EMPTY_PAGE);
    const Page = await loadPage();
    renderToStaticMarkup(await Page(pageProps({ fleet: value })));
    // No `fleet` key at all. An empty value reaches the daemon as an unbounded
    // filter, which it refuses with a 400 — rendering an error state for what is
    // really just a malformed link.
    expect(listRunnerLeasesMock).toHaveBeenCalledWith("tok", RUNNER.id, { limit: 25 });
  });
});
