import { afterEach, describe, expect, it } from "vitest";
import { cleanup, screen } from "@testing-library/react";
import { entry, renderView } from "@/tests/helpers/fleet-library-view";

afterEach(cleanup);

describe("Fleet library source rendering", () => {
  it("links the repository to GitHub when the source is owner/repo shaped", async () => {
    renderView([entry({ id: "linked", source_repo: "agentsfleet/platform-ops" })]);

    const link = await screen.findByRole("link", { name: /agentsfleet\/platform-ops/ });
    // Pinned to the ref the row was fetched at, not the repository root: two
    // entries off the same repository at different refs are different bundles,
    // and a link to the default branch would say they are the same.
    expect(link.getAttribute("href")).toBe(
      "https://github.com/agentsfleet/platform-ops/tree/main",
    );
    // Never a tab-hijack: external link opens away without a window handle.
    expect(link.getAttribute("rel")).toContain("noopener");
  });

  it("links the repository root for a row that stores no ref", async () => {
    // `source_ref` is NOT NULL but may hold the empty string, and
    // `/tree/` with nothing after it is a 404 on github.com. The row falls
    // back to the repository root rather than building that URL.
    renderView([
      entry({ id: "refless", source_repo: "agentsfleet/platform-ops", source_ref: "" }),
    ]);

    const link = await screen.findByRole("link", { name: /agentsfleet\/platform-ops/ });
    expect(link.getAttribute("href")).toBe("https://github.com/agentsfleet/platform-ops");
    // And no "@" dangling where the ref would have been.
    expect(link.textContent).toBe("agentsfleet/platform-ops");
  });

  it("draws an uploaded row as an upload, never a repository", async () => {
    // The shape an upload ACTUALLY stores: the empty string (catalog-status.ts
    // — "An upload stores the empty string, so there is no revision to
    // re-read"). The other non-slug case in this file is a pasted string; this
    // is the one that occurs in production, and `sourceKindOf` keys the glyph
    // off the same predicate `rowActions` keys Fetch off, so a row drawn as a
    // repository here would also be offered a refetch it cannot serve.
    renderView([entry({ id: "uploaded", name: "uploaded-bundle", source_repo: "", source_ref: "" })]);

    expect(await screen.findAllByText("uploaded-bundle")).toHaveLength(2);
    expect(screen.queryByRole("link", { name: /Open on GitHub/ })).toBeNull();
  });

  // A template- or upload-sourced row carries a source that is not a GitHub
  // slug. Linking it would point at a repository that does not exist — inert
  // text is the honest rendering.
  it("renders a non-slug source as inert text, never a broken link", async () => {
    renderView([entry({ id: "pasted", source_repo: "platform/template:ops" })]);

    expect(await screen.findByText("platform/template:ops")).toBeTruthy();
    expect(screen.queryByRole("link", { name: /platform\/template:ops/ })).toBeNull();
  });
});
