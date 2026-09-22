/**
 * Where a bundle came from, drawn once for both fleet-library tables.
 *
 * Two properties carry the weight here. The kind is never a word in the cell —
 * it used to be a `upload:` / `github:` prefix on every row, and the whole
 * point of the glyph is that the prefix is gone. And a link is only ever
 * offered for a source that can be resolved: an upload stores a local path, so
 * linking it would send an operator to a repository that does not exist.
 */
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import {
  SOURCE_KIND_GITHUB,
  SOURCE_KIND_TEMPLATE,
  SOURCE_KIND_UPLOAD,
  SourceMark,
  githubSourceUrl,
} from "./SourceMark";

afterEach(cleanup);

describe("a source is drawn as its kind's mark", () => {
  it("draws the GitHub mark and links the repository", () => {
    const { container } = render(
      <SourceMark kind={SOURCE_KIND_GITHUB} sourceRef="agentsfleet/github-pr-reviewer" />,
    );

    expect(container.querySelector("[data-vendor-mark='github']")).toBeTruthy();
    const link = screen.getByRole("link", { name: /agentsfleet\/github-pr-reviewer/ });
    expect(link.getAttribute("href")).toBe("https://github.com/agentsfleet/github-pr-reviewer");
    // Never a tab-hijack: an external link opens away without a window handle.
    expect(link.getAttribute("rel")).toContain("noopener");
  });

  it("pins the link to the ref the row was fetched at, when it stores one", () => {
    render(
      <SourceMark
        kind={SOURCE_KIND_GITHUB}
        sourceRef="agentsfleet/github-pr-reviewer"
        gitRef="v1.2.0"
      />,
    );

    const link = screen.getByRole("link", { name: /github-pr-reviewer@v1\.2\.0/ });
    expect(link.getAttribute("href")).toBe(
      "https://github.com/agentsfleet/github-pr-reviewer/tree/v1.2.0",
    );
    // A branch link is a link to whatever that branch is today, and the cell
    // says so rather than implying the row is pinned.
    expect(link.getAttribute("title")).toContain("branch name, not the commit");
  });

  it("never spells the kind as a prefix", () => {
    render(<SourceMark kind={SOURCE_KIND_UPLOAD} sourceRef="probe/skill-only" />);

    expect(screen.queryByText("upload:probe/skill-only")).toBeNull();
    expect(screen.getByText("probe/skill-only")).toBeTruthy();
  });

  it("leaves an upload as inert text, never a broken link", () => {
    render(<SourceMark kind={SOURCE_KIND_UPLOAD} sourceRef="agentsfleet/looks-like-a-repo" />);

    // Slug-SHAPED and still an upload: the kind decides, not the shape, so a
    // path that happens to read as `owner/repo` is not promoted to a link.
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("refuses to link a GitHub row whose ref is not a repository slug", () => {
    render(<SourceMark kind={SOURCE_KIND_GITHUB} sourceRef="platform/template:ops" />);

    expect(screen.queryByRole("link")).toBeNull();
    expect(screen.getByText("platform/template:ops")).toBeTruthy();
  });

  it("draws a template with its own mark rather than the upload one", () => {
    const { container: template } = render(
      <SourceMark kind={SOURCE_KIND_TEMPLATE} sourceRef="platform-ops" />,
    );
    const templateGlyph = template.querySelector("svg")?.getAttribute("class");

    cleanup();

    const { container: upload } = render(
      <SourceMark kind={SOURCE_KIND_UPLOAD} sourceRef="platform-ops" />,
    );
    const uploadGlyph = upload.querySelector("svg")?.getAttribute("class");

    expect(templateGlyph).toBeTruthy();
    expect(templateGlyph).not.toBe(uploadGlyph);
  });

  it("falls back to the upload mark for a kind it has never seen", () => {
    // A kind the server grows and this file has not learned about must still
    // render a row, not an empty cell or a thrown component.
    render(<SourceMark kind="gitlab" sourceRef="acme/reviewer" />);

    expect(screen.getByText("acme/reviewer")).toBeTruthy();
    expect(screen.queryByRole("link")).toBeNull();
  });
});

describe("githubSourceUrl", () => {
  it("builds the repository root when no ref is given", () => {
    expect(githubSourceUrl("acme/reviewer")).toBe("https://github.com/acme/reviewer");
  });

  it("builds a tree link at the ref when one is given", () => {
    expect(githubSourceUrl("acme/reviewer", "main")).toBe(
      "https://github.com/acme/reviewer/tree/main",
    );
  });
});
