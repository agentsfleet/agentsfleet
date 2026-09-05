import { EMPTY_PAGE, GRAFANA_ENV, NOT_ADMIN, RUNNER, getRunnerMock, hasScopeMock, listRunnerEventsMock, listRunnerLeasesMock, loadPage, mockAuth, notFound, pageProps, redirect } from "./harness";
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ApiError } from "@/lib/api/errors";

describe("admin/runners/[runnerId] page — guards and failure handling", () => {
  it("redirects a caller without runner:read before any read happens", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow(`redirect:${NOT_ADMIN}`);
    expect(getRunnerMock).not.toHaveBeenCalled();
  });

  it("redirects to /sign-in when the admin session has no token", async () => {
    mockAuth(null);
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow("redirect:/sign-in");
  });

  it("renders notFound for an unknown runner id", async () => {
    mockAuth();
    getRunnerMock.mockRejectedValueOnce(new ApiError("no runner", 404, "UZ-RUN-014"));
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow("notFound");
  });

  it("redirects to settings when the backend independently 403s the read", async () => {
    mockAuth();
    getRunnerMock.mockRejectedValueOnce(new ApiError("forbidden", 403, "UZ-AUTH-022"));
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow(`redirect:${NOT_ADMIN}`);
  });

  it("redirects to /sign-in when the backend returns 401", async () => {
    mockAuth();
    getRunnerMock.mockRejectedValueOnce(new ApiError("expired", 401, "UZ-AUTH-401"));
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow("redirect:/sign-in");
  });

  it("re-throws a non-403/401 ApiError instead of redirecting", async () => {
    mockAuth();
    getRunnerMock.mockRejectedValueOnce(new ApiError("exploded", 500, "UZ-INTERNAL-001"));
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow("exploded");
  });

  it("runner detail starts the view read beside the runner read", async () => {
    mockAuth();
    let releaseRunner: (runner: typeof RUNNER) => void = () => {};
    getRunnerMock.mockReturnValueOnce(
      new Promise<typeof RUNNER>((resolve) => {
        releaseRunner = resolve;
      }),
    );
    const Page = await loadPage();
    const rendering = Page(pageProps());
    // The lease read is on the wire while the runner read is still pending —
    // one round-trip, not two in sequence.
    await vi.waitFor(() => expect(listRunnerLeasesMock).toHaveBeenCalledWith("tok", RUNNER.id, { limit: 25 }));
    expect(getRunnerMock).toHaveBeenCalledTimes(1);
    releaseRunner(RUNNER);
    const html = renderToStaticMarkup(await rendering);
    expect(html).toContain('data-lease-table="0"');
  });

  it("runner detail failure handling is unchanged by the parallel start", async () => {
    // Not-found still short-circuits the page even though the lease read was
    // already issued, and a failed lease read still renders its warning.
    mockAuth();
    getRunnerMock.mockRejectedValueOnce(new ApiError("no runner", 404, "UZ-RUN-014"));
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow("notFound");
    expect(listRunnerLeasesMock).toHaveBeenCalledTimes(1);

    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockRejectedValueOnce(new Error("lease read down"));
    const html = renderToStaticMarkup(await Page(pageProps()));
    // Resolved here, not at module top: a static import of the copy module would
    // load the mocked runners module before the hoisted mock factory's consts exist.
    const { LEASES_UNAVAILABLE } = await import(
      "@/app/(dashboard)/admin/runners/[runnerId]/components/runner-copy"
    );
    expect(html).toContain(LEASES_UNAVAILABLE);
    expect(html).toContain('data-runner-strip="1"');
  });

  it("a view read that fails while the runner read fails leaves no unhandled rejection", async () => {
    // The lease read was already issued beside the runner read; when both fail
    // the page still ends in not-found, and the lease rejection is absorbed —
    // vitest fails the file on an unhandled rejection, so this case is
    // load-bearing for the catch on the parallel start.
    mockAuth();
    getRunnerMock.mockRejectedValueOnce(new ApiError("no runner", 404, "UZ-RUN-014"));
    listRunnerLeasesMock.mockRejectedValueOnce(new Error("lease read down"));
    const Page = await loadPage();
    await expect(Page(pageProps())).rejects.toThrow("notFound");
    expect(listRunnerLeasesMock).toHaveBeenCalledTimes(1);
  });

  it("says the history is unavailable when a view read errors, never an empty history", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockRejectedValueOnce(new Error("lease read down"));
    const Page = await loadPage();
    const html = renderToStaticMarkup(await Page(pageProps()));
    // An empty table would read as "this host has never held a lease", which is
    // the opposite of what happened. The shell and the strip still render.
    expect(html).toContain("Lease history is temporarily unavailable");
    expect(html).not.toContain("data-lease-table");
    expect(html).toContain('data-runner-strip="1"');

    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerEventsMock.mockRejectedValueOnce(new Error("events read down"));
    const activityHtml = renderToStaticMarkup(await Page(pageProps({ view: "activity" })));
    expect(activityHtml).toContain("Activity history is temporarily unavailable");
    expect(activityHtml).not.toContain("data-activity-table");
  });

  it("offers a way out when the server refuses the address, instead of telling the operator to refresh", async () => {
    // A 400 is the URL's fault, not the server's: a hand-edited workspace
    // filter, or a bookmarked cursor whose lease retention has since deleted.
    // Refreshing replays the same bad address forever, and the control that
    // could clear the filter lives inside the table — which is exactly what
    // does not render on a failed read. Without a link here the page is a dead
    // end reachable from a stale bookmark.
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockRejectedValueOnce(
      new ApiError("workspace_id must be a workspace id", 400, "UZ-REQ-001"),
    );
    const Page = await loadPage();
    const html = renderToStaticMarkup(
      await Page(pageProps({ workspace: "not-a-uuid" })),
    );

    expect(html).toContain("its workspace filter or page cursor is no longer valid");
    expect(html).toContain("Show the newest leases instead");
    expect(html).toContain(`href="/admin/runners/${RUNNER.id}"`);
    // Not the transient copy: that one says to refresh, which cannot work here.
    expect(html).not.toContain("Lease history is temporarily unavailable");
    expect(html).not.toContain("data-lease-table");
    expect(html).toContain('data-runner-strip="1"');
  });

  it("keeps the try-refreshing copy for a genuinely transient failure", async () => {
    // The counterpart to the test above: a read that failed rather than one
    // that was refused. Refreshing IS the right move here, so the recovery link
    // must not appear — otherwise every blip invites the operator to throw away
    // the filter they meant to keep.
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockRejectedValueOnce(
      new ApiError("upstream unavailable", 503, "UZ-INTERNAL-002"),
    );
    const Page = await loadPage();
    const html = renderToStaticMarkup(await Page(pageProps()));

    expect(html).toContain("Lease history is temporarily unavailable");
    expect(html).not.toContain("Show the newest leases instead");
  });

  it("builds the Grafana link only against a configured base, with the runner filter appended", async () => {
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockResolvedValueOnce(EMPTY_PAGE);
    process.env[GRAFANA_ENV] = "https://grafana.example/d/runners";
    const Page = await loadPage();
    const html = renderToStaticMarkup(await Page(pageProps()));
    expect(html).toContain(
      `data-grafana="https://grafana.example/d/runners?var-runner_id=${RUNNER.id}"`,
    );

    // A base already carrying a query joins with & instead of a second ?.
    mockAuth();
    getRunnerMock.mockResolvedValueOnce(RUNNER);
    listRunnerLeasesMock.mockResolvedValueOnce(EMPTY_PAGE);
    process.env[GRAFANA_ENV] = "https://grafana.example/d/runners?orgId=1";
    const withQuery = renderToStaticMarkup(await Page(pageProps()));
    // renderToStaticMarkup HTML-escapes the ampersand in the attribute.
    expect(withQuery).toContain(
      `data-grafana="https://grafana.example/d/runners?orgId=1&amp;var-runner_id=${RUNNER.id}"`,
    );
  });
});
