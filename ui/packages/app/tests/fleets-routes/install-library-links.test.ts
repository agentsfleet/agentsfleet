import { SAMPLE_TEMPLATES } from "./harness";
import React from "react";
import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { listWorkspaceFleetLibraryMock, listSecretsMock } from "../helpers/dashboard-app-mocks";

describe("fleets routes — install library paging and deep links", () => {
  it("test_library_list_position_survives_reload — a cursor in the URL restores that page", async () => {
    // The server half of list position. Load-more mirrors the cursor into the
    // URL; this is what makes that mirror worth anything — a reload, a shared
    // link, or Back out of a detail view must land on the page the user was
    // on, not dump them at the first one having lost their scroll.
    listWorkspaceFleetLibraryMock.mockResolvedValue({
      items: SAMPLE_TEMPLATES,
      next_cursor: null,
      total: 2,
    });
    listSecretsMock.mockResolvedValue({ secrets: [] });

    const { InstallFleetData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await InstallFleetData({ workspaceId: "ws_1", query: { library_after: "cur-2" } }),
      ),
    );

    // The cursor reached the read verbatim — not dropped, not re-encoded.
    expect(listWorkspaceFleetLibraryMock.mock.calls[0]?.[2]).toBe("cur-2");
    expect(markup).toContain("GitHub PR reviewer");
    // One request: restoring a position must not walk from page one to find it.
    expect(listWorkspaceFleetLibraryMock).toHaveBeenCalledTimes(1);
  });

  it("falls back to the first page when a supplied cursor is rejected", async () => {
    // A stale or hand-edited ?library_after must not strand someone on an
    // error screen. The first page always lands somewhere useful.
    const rejected = Object.assign(new Error("bad cursor"), { status: 400 });
    listWorkspaceFleetLibraryMock
      .mockRejectedValueOnce(rejected)
      .mockResolvedValueOnce({ items: SAMPLE_TEMPLATES, next_cursor: null, total: 2 });

    const { InstallFleetData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await InstallFleetData({ workspaceId: "ws_1", query: { library_after: "garbage" } }),
      ),
    );

    // Second call carries no cursor — that is the fallback, not a retry of the
    // same bad request.
    expect(listWorkspaceFleetLibraryMock).toHaveBeenCalledTimes(2);
    expect(listWorkspaceFleetLibraryMock.mock.calls[1]?.[2] ?? null).toBeNull();
    expect(markup).toContain("GitHub PR reviewer");
    expect(markup).not.toContain("Could not load the fleet library.");
  });

  it("classifies the FALLBACK's failure when the first-page retry also fails", async () => {
    // A stale cursor 400 followed by a 503 on the fallback is an availability
    // incident, not a cursor problem — reporting the discarded 400 would show
    // a cursor-shaped error while the library is down.
    const badCursor = Object.assign(new Error("bad cursor"), { status: 400 });
    const down = Object.assign(new Error("upstream"), { status: 503 });
    listWorkspaceFleetLibraryMock
      .mockRejectedValueOnce(badCursor)
      .mockRejectedValueOnce(down);
    listSecretsMock.mockResolvedValue({ secrets: [] });

    const { InstallFleetData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await InstallFleetData({ workspaceId: "ws_1", query: { library_after: "stale" } }),
      ),
    );
    expect(markup).toContain("The fleet library is temporarily unavailable.");
  });

  it("does not retry a first-page failure that no cursor could fix", async () => {
    // Only a REJECTED CURSOR earns the fallback. A 503 on the first page is a
    // real outage, and a silent second round-trip would just double the load.
    const down = Object.assign(new Error("upstream"), { status: 503 });
    listWorkspaceFleetLibraryMock.mockRejectedValue(down);
    listSecretsMock.mockResolvedValue({ secrets: [] });

    const { InstallFleetData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await InstallFleetData({ workspaceId: "ws_1", query: {} }),
      ),
    );

    expect(listWorkspaceFleetLibraryMock).toHaveBeenCalledTimes(1);
    expect(markup).toContain("The fleet library is temporarily unavailable.");
  });

  it("test_fleet_deep_link_and_typed_states — tier-qualified deep link and exact states", async () => {
    listWorkspaceFleetLibraryMock.mockResolvedValue({ items: [], next_cursor: null, total: 0 });
    listSecretsMock.mockResolvedValue({ secrets: [] });
    const { InstallFleetData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await InstallFleetData({
          workspaceId: "ws_1",
          query: { library_visibility: "platform", library_id: "github-pr-reviewer" },
        }),
      ),
    );
    expect(markup).toContain("Fleet library");
    // Absent from the loaded page → the not-found selection state, which
    // neither enumerates nor errors the page.
    expect(markup).toContain("not on this page");
  });

  it("a deep link whose entry IS on the page resolves straight to the install states", async () => {
    // The resolver's positive path. An inverted or typo'd match predicate
    // would turn every valid deep link into "not on this page" with the rest
    // of the suite green — this is the assertion that catches it.
    listWorkspaceFleetLibraryMock.mockResolvedValue({ items: SAMPLE_TEMPLATES, next_cursor: null, total: null });
    listSecretsMock.mockResolvedValue({ secrets: [] });
    const { InstallFleetData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await InstallFleetData({
          workspaceId: "ws_1",
          query: { library_visibility: "platform", library_id: "github-pr-reviewer" },
        }),
      ),
    );
    // One-step install, first paint: the states shell is present, the gallery
    // is not — no frame in which the gallery is wrong, and no confirm step.
    expect(markup).toContain("Install states");
    expect(markup).not.toContain("Fleet library");
    expect(markup).not.toContain("not on this page");
  });

  it("a deep link matching the id under the OTHER tier is a not-found selection", async () => {
    // The create body keys off the tier, so a tier mismatch must not resolve.
    listWorkspaceFleetLibraryMock.mockResolvedValue({ items: SAMPLE_TEMPLATES, next_cursor: null, total: null });
    listSecretsMock.mockResolvedValue({ secrets: [] });
    const { InstallFleetData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await InstallFleetData({
          workspaceId: "ws_1",
          query: { library_visibility: "tenant", library_id: "github-pr-reviewer" },
        }),
      ),
    );
    expect(markup).toContain("not on this page");
    expect(markup).not.toContain("Fleet name");
  });

  it.each([
    // Apostrophes are asserted-around, not spelled: renderToStaticMarkup
    // HTML-escapes them (&#x27;).
    [401, "Your session expired. Sign in to browse the fleet library."],
    [403, "You do not have access to this workspace"],
  ])("a %i on the gallery read renders its own instruction, not a generic failure", async (status, copy) => {
    const rejected = Object.assign(new Error("denied"), { status });
    listWorkspaceFleetLibraryMock.mockRejectedValue(rejected);
    listSecretsMock.mockResolvedValue({ secrets: [] });
    const { InstallFleetData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/new/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await InstallFleetData({ workspaceId: "ws_1", query: {} }),
      ),
    );
    expect(markup).toContain(copy);
    expect(markup).not.toContain("No prebuilt fleet library found");
  });
});
