import { SAMPLE_TEMPLATES } from "./harness";
import React from "react";
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { authMock as auth } from "../helpers/dashboard-mocks";
import { listWorkspaceFleetLibraryMock, listSecretsMock } from "../helpers/dashboard-app-mocks";

describe("fleets routes — install page", () => {
  it("fleets new page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    await expect(
      Page({
        params: Promise.resolve({ workspaceId: "ws_1" }),
        searchParams: Promise.resolve({}),
      }),
    ).rejects.toThrow("redirect:/sign-in");
  });

  it("fleets new page shell streams the header + skeleton before the gallery", async () => {
    // InstallFleetData is an async child, so renderToStaticMarkup renders the
    // Suspense skeleton in its place — the header paints immediately and the
    // gallery arrives after. Previously the whole page waited on the slower of
    // the library and vault reads before painting anything.
    listWorkspaceFleetLibraryMock.mockResolvedValue({ items: SAMPLE_TEMPLATES, next_cursor: null, total: null });
    listSecretsMock.mockResolvedValue({ secrets: [] });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1" }),
        searchParams: Promise.resolve({}),
      }),
    );
    expect(markup).toContain("Install fleet"); // PageTitle in the shell
    expect(markup).toContain("animate-pulse"); // Skeleton fallback
    expect(markup).not.toContain("GitHub PR reviewer"); // data not yet resolved
  });

  it("fleets new page renders the gallery-first install flow when a workspace exists", async () => {
    listWorkspaceFleetLibraryMock.mockResolvedValue({
      items: SAMPLE_TEMPLATES,
      next_cursor: null,
      total: null,
    });
    listSecretsMock.mockResolvedValue({
      secrets: [{ kind: "custom_secret", name: "github", created_at: 1 }],
    });
    const { InstallFleetData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await InstallFleetData({ workspaceId: "ws_1", query: {} }),
      ),
    );
    expect(markup).toContain("Fleet library");
    expect(markup).toContain("GitHub PR reviewer");
    expect(markup).toContain("Install"); // the gallery card's install action
  });

  it("fleets new page surfaces a failed library read instead of an empty gallery", async () => {
    // This replaces a test named "swallows failed template + secret fetches",
    // which asserted the anti-pattern: `.catch(() => [])` told a workspace its
    // library was empty when the read had merely failed, with no retry and no
    // way to tell the two apart. A failed read is now a failure.
    listWorkspaceFleetLibraryMock.mockRejectedValue(new Error("catalog down"));
    listSecretsMock.mockRejectedValue(new Error("vault down"));
    const { InstallFleetData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await InstallFleetData({ workspaceId: "ws_1", query: {} }),
      ),
    );
    expect(markup).toContain("Could not load the fleet library.");
    expect(markup).not.toContain("No prebuilt fleet library found");
  });

  it("the streamed gallery renders nothing at all when the session lapsed mid-stream", async () => {
    // The shell already redirected on a missing token, but the gallery is a
    // SEPARATE async child that mints its own. A session that lapses between
    // the two reads must yield no gallery rather than an unauthenticated read
    // — and it cannot redirect from here, because the shell has already been
    // flushed to the browser.
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { InstallFleetData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await InstallFleetData({ workspaceId: "ws_1", query: {} }),
      ),
    );
    expect(markup).toBe("");
    expect(listWorkspaceFleetLibraryMock).not.toHaveBeenCalled();
  });

  it("a repeated query parameter resolves to its first value, not to the array", async () => {
    // `?library_after=a&library_after=b` reaches Next as an array. Passing it
    // on unchanged would build a cursor of "a,b" and read a page the server
    // never issued — a hand-edited or duplicated share link, not a hostile
    // one, and it must land on a real page rather than an error.
    listWorkspaceFleetLibraryMock.mockResolvedValue({
      items: SAMPLE_TEMPLATES,
      next_cursor: null,
      total: null,
    });
    listSecretsMock.mockResolvedValue({ secrets: [] });
    const { InstallFleetData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await InstallFleetData({ workspaceId: "ws_1", query: { library_after: ["cur-2", "cur-3"] } }),
      ),
    );

    expect(markup).toContain("GitHub PR reviewer");
    expect(listWorkspaceFleetLibraryMock).toHaveBeenCalledWith("ws_1", expect.anything(), "cur-2");
  });
});
