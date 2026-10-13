/**
 * A platform entry cannot appear on this page, and the reason is the request.
 *
 * The page reads the OWNED collection, which never carries a platform row, so
 * the exclusion is a property of what was asked for rather than a filter that
 * could drift. That makes the assertion a claim about the path issued, not
 * about what rendered — a rendering test would pass just as happily if the
 * page read the gallery and filtered client-side, which is the design this
 * spec rejected.
 */
import { describe, expect, it, vi } from "vitest";

const { listCachedMock, requireCredentialMock } = vi.hoisted(() => ({
  listCachedMock: vi.fn(),
  requireCredentialMock: vi.fn(),
}));
vi.mock("@/lib/auth/credential", () => ({ requireCredential: requireCredentialMock }));
vi.mock("./lib/reads", () => ({ listLibraryEntriesCached: listCachedMock }));

import type { ReactElement } from "react";
import LibraryPage from "./page";
import { LIBRARY_SECTION_LABEL } from "./copy";

type Props = { "aria-label"?: string; className?: string; children?: unknown };

/** The element in `tree` whose accessible name is `label`, searched depth first. */
function labelled(tree: unknown, label: string): ReactElement<Props> | undefined {
  if (Array.isArray(tree)) {
    return tree.map((child) => labelled(child, label)).find(Boolean);
  }
  if (tree === null || typeof tree !== "object" || !("props" in tree)) return undefined;
  const element = tree as ReactElement<Props>;
  if (element.props["aria-label"] === label) return element;
  return labelled(element.props.children, label);
}

describe("the library page's read", () => {
  it("reads the owned collection for the workspace in the path", async () => {
    requireCredentialMock.mockResolvedValue("token-1");
    listCachedMock.mockResolvedValue({ items: [], total: null, next_cursor: null });

    await LibraryPage({ params: Promise.resolve({ workspaceId: "ws_1" }) });

    expect(listCachedMock).toHaveBeenCalledWith("ws_1", "token-1");
  });

  it("lets the section shrink to the page without stretching it", async () => {
    requireCredentialMock.mockResolvedValue("token-1");
    listCachedMock.mockResolvedValue({ items: [], total: null, next_cursor: null });

    const tree = await LibraryPage({ params: Promise.resolve({ workspaceId: "ws_1" }) });

    const classes = labelled(tree, LIBRARY_SECTION_LABEL)?.props.className?.split(" ") ?? [];
    expect(classes).toContain("min-h-0");
    expect(classes).not.toContain("flex-1");
  });

  it("hands the list exactly what the server answered", async () => {
    // No filtering, mapping or re-ordering between the read and the table:
    // whatever else changes, the page must not become the place where the
    // tier rule is enforced.
    const items = [
      {
        id: "e1",
        name: "github-pr-reviewer",
        description: "Reviews pull requests.",
        source_kind: "github",
        source_ref: "acme/reviewer",
        content_hash: "abc",
        created_at: 1_777_507_200_000,
      },
    ];
    requireCredentialMock.mockResolvedValue("token-1");
    listCachedMock.mockResolvedValue({ items, total: null, next_cursor: null });

    const tree = await LibraryPage({ params: Promise.resolve({ workspaceId: "ws_1" }) });

    expect(JSON.stringify(tree)).toContain("github-pr-reviewer");
  });
});
