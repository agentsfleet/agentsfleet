/**
 * The workspace's own entries, and taking one out.
 *
 * The rule worth asserting is what CANNOT appear: a platform entry. It cannot,
 * because the page reads the owned collection and this component renders what
 * it was handed — there is no client-side filter to drift. So the test that
 * matters is the one on the page's request, which lives beside this file in
 * `page.test.tsx`; here the subject is the removal, which is the destructive
 * half and the one with a confirmation in front of it.
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { WorkspaceLibraryEntry } from "@/lib/api/library-types";

const { refreshMock, removeActionMock, listActionMock } = vi.hoisted(() => ({
  refreshMock: vi.fn(),
  removeActionMock: vi.fn(),
  listActionMock: vi.fn(),
}));
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: refreshMock, push: vi.fn() }),
}));
vi.mock("../actions", () => ({
  removeLibraryEntryAction: removeActionMock,
  listLibraryEntriesAction: listActionMock,
}));

import WorkspaceLibraryList from "./WorkspaceLibraryList";

const CREATED_MS = Date.UTC(2026, 3, 30, 10, 30, 0);

function entry(id: string, name: string, createdAt = CREATED_MS): WorkspaceLibraryEntry {
  return {
    id,
    name,
    description: "Reviews pull requests.",
    source_kind: "github",
    source_ref: "acme/reviewer",
    content_hash: `hash-${id}`,
    created_at: createdAt,
  };
}

/** The entry names the table renders, top row first. */
function renderedNames(): string[] {
  return screen
    .getAllByRole("row")
    .slice(1)
    .map((row) => row.querySelectorAll("td")[0]?.textContent ?? "");
}

function renderList(entries: WorkspaceLibraryEntry[], initialCursor: string | null = null) {
  return render(
    React.createElement(
      TooltipProvider,
      null,
      React.createElement(WorkspaceLibraryList, { workspaceId: "ws_1", entries, initialCursor }),
    ),
  );
}

afterEach(() => {
  cleanup();
  refreshMock.mockReset();
  removeActionMock.mockReset();
  listActionMock.mockReset();
});

describe("the workspace library list", () => {
  it("renders a row per onboarded entry, with its provenance", () => {
    renderList([entry("e1", "github-pr-reviewer"), entry("e2", "incident-responder")]);

    expect(screen.getByText("github-pr-reviewer")).toBeTruthy();
    expect(screen.getByText("incident-responder")).toBeTruthy();
    // Two onboardings of near-identical bundles differ by source first.
    expect(screen.getAllByText("github:acme/reviewer")).toHaveLength(2);
  });

  it("shows an empty state naming the command that fills it", () => {
    // An empty list with no next step reads as a broken screen rather than an
    // empty one, and `library add` is what fills it.
    renderList([]);

    expect(screen.getByText(/library add/)).toBeTruthy();
    expect(screen.queryByRole("table")).toBeNull();
  });

  it("asks before removing, and names the entry in the question", async () => {
    renderList([entry("e1", "github-pr-reviewer")]);

    fireEvent.click(screen.getAllByRole("button", { name: "Remove" })[0]!);

    await waitFor(() => {
      expect(screen.getByText(/Remove "github-pr-reviewer" from this workspace\?/)).toBeTruthy();
    });
    // Opening the question sends nothing.
    expect(removeActionMock).not.toHaveBeenCalled();
  });

  it("says what survives the removal", async () => {
    // Someone reading the confirmation should not have to guess whether a
    // running fleet is about to stop. It is not, and the copy says so.
    renderList([entry("e1", "github-pr-reviewer")]);
    fireEvent.click(screen.getAllByRole("button", { name: "Remove" })[0]!);

    await waitFor(() => {
      expect(screen.getByText(/keep running/)).toBeTruthy();
    });
  });

  it("removes the entry the dialog named, once confirmed", async () => {
    removeActionMock.mockResolvedValue({ ok: true });
    renderList([entry("e1", "github-pr-reviewer"), entry("e2", "incident-responder")]);

    fireEvent.click(screen.getAllByRole("button", { name: "Remove" })[0]!);
    await waitFor(() => screen.getByText(/from this workspace\?/));
    // The dialog's own confirm, not the row action that opened it.
    const confirms = screen.getAllByRole("button", { name: "Remove" });
    fireEvent.click(confirms[confirms.length - 1]!);

    await waitFor(() => {
      expect(removeActionMock).toHaveBeenCalledWith("ws_1", "e1");
    });
    await waitFor(() => expect(refreshMock).toHaveBeenCalled());
  });

  it("leaves the row and surfaces the refusal when removal fails", async () => {
    // A refused removal must not look like a successful one. The row comes
    // back when the transition ends, and the sentence the server sent is what
    // the operator reads.
    removeActionMock.mockResolvedValue({
      ok: false,
      status: 500,
      error: "the datastore would not answer",
      errorCode: "UZ-LIBRARY-006",
    });
    renderList([entry("e1", "github-pr-reviewer")]);

    fireEvent.click(screen.getAllByRole("button", { name: "Remove" })[0]!);
    await waitFor(() => screen.getByText(/from this workspace\?/));
    const confirms = screen.getAllByRole("button", { name: "Remove" });
    fireEvent.click(confirms[confirms.length - 1]!);

    await waitFor(() => {
      expect(screen.getByText(/datastore would not answer/)).toBeTruthy();
    });
    // The row comes back when the transition ends, not when the action
    // returns — React restores it from the server-rendered list on its own,
    // which is why nothing here has to put it back.
    await waitFor(() => {
      expect(screen.getByText("github-pr-reviewer")).toBeTruthy();
    });
  });
  it("should order by name when the Name header is sorted", async () => {
    // The column is sortable because it carries a `sortValue`, so the callback
    // is what decides the order an operator sees. A sort key reading the wrong
    // field looks identical until two rows disagree, which is why the fixture
    // is seeded out of order.
    renderList([entry("e1", "incident-responder"), entry("e2", "github-pr-reviewer")]);
    expect(renderedNames()).toEqual(["incident-responder", "github-pr-reviewer"]);

    fireEvent.click(screen.getByRole("button", { name: /Name/ }));

    await waitFor(() => {
      expect(renderedNames()).toEqual(["github-pr-reviewer", "incident-responder"]);
    });
  });

  it("should order by onboarding time when the Onboarded header is sorted", async () => {
    // Three near-identical entries under one name is the pile-up this page
    // exists to clear, and then the only thing telling them apart is when each
    // arrived — so this sort key is the one that has to be the timestamp.
    const older = CREATED_MS - 86_400_000;
    renderList([entry("e1", "github-pr-reviewer"), entry("e2", "incident-responder", older)]);
    expect(renderedNames()).toEqual(["github-pr-reviewer", "incident-responder"]);

    fireEvent.click(screen.getByRole("button", { name: /Onboarded/ }));

    await waitFor(() => {
      expect(renderedNames()).toEqual(["incident-responder", "github-pr-reviewer"]);
    });
  });

  it("should issue no request and keep the row when the question is dismissed", async () => {
    // Dimension 3.2's claim, which no test held until now: backing out of a
    // destructive confirmation must be free. A dismiss wired to the confirm
    // path would remove the entry on Escape.
    renderList([entry("e1", "github-pr-reviewer")]);
    fireEvent.click(screen.getAllByRole("button", { name: "Remove" })[0]!);
    await waitFor(() => screen.getByText(/from this workspace\?/));

    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape", code: "Escape" });

    await waitFor(() => {
      expect(screen.queryByText(/from this workspace\?/)).toBeNull();
    });
    expect(removeActionMock).not.toHaveBeenCalled();
    expect(refreshMock).not.toHaveBeenCalled();
    expect(screen.getByText("github-pr-reviewer")).toBeTruthy();
  });
  it("should show the bare source reference when an entry carries no kind", () => {
    // An upload onboarded without a kind still has to say where it came from.
    // The falsy arm printing `undefined:acme/reviewer` is the visible bug.
    renderList([{ ...entry("e1", "github-pr-reviewer"), source_kind: "" }]);

    expect(screen.getByText("acme/reviewer")).toBeTruthy();
    expect(screen.queryByText(/^:/)).toBeNull();
  });

  it("should not re-read the list when the server definitely refused", async () => {
    // A 4xx is the server's final answer, so the row the transition restores
    // is already true and a refresh would be a wasted round trip. A 5xx or a
    // timeout is NOT definite — the removal may have landed — and that path
    // does refresh, which the 500 case above covers.
    removeActionMock.mockResolvedValue({
      ok: false,
      status: 403,
      error: "you do not hold library:write",
      errorCode: "UZ-AUTH-003",
    });
    renderList([entry("e1", "github-pr-reviewer")]);

    fireEvent.click(screen.getAllByRole("button", { name: "Remove" })[0]!);
    await waitFor(() => screen.getByText(/from this workspace\?/));
    const confirms = screen.getAllByRole("button", { name: "Remove" });
    fireEvent.click(confirms[confirms.length - 1]!);

    await waitFor(() => {
      expect(screen.getByText(/do not hold library:write/)).toBeTruthy();
    });
    expect(refreshMock).not.toHaveBeenCalled();
  });
});

/**
 * Reading one page and rendering `items` drops every entry past the server's
 * page size with nothing on screen to say so. The gallery beside this list
 * documents that hazard as unacceptable and pages behind a control; so does
 * the runner wall. These are the cases that keep this list honest about what
 * it has not loaded.
 */
describe("the list discloses what it has not loaded", () => {
  const CURSOR = "cursor-page-2";

  it("should offer no control when the first page is the whole collection", () => {
    renderList([entry("e1", "github-pr-reviewer")], null);
    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
  });

  it("should offer a control when the server says a page remains", () => {
    renderList([entry("e1", "github-pr-reviewer")], CURSOR);
    expect(screen.getByRole("button", { name: "Load more" })).toBeTruthy();
  });

  it("should append the next page and follow its cursor", async () => {
    listActionMock.mockResolvedValue({
      ok: true,
      data: { items: [entry("e2", "incident-responder")], total: null, next_cursor: null },
    });
    renderList([entry("e1", "github-pr-reviewer")], CURSOR);

    fireEvent.click(screen.getByRole("button", { name: "Load more" }));

    await waitFor(() => expect(screen.getByText("incident-responder")).toBeTruthy());
    expect(listActionMock).toHaveBeenCalledWith("ws_1", CURSOR);
    expect(renderedNames()).toEqual(["github-pr-reviewer", "incident-responder"]);
    // The cursor came back null, so the collection is exhausted and the
    // control goes — a button that fetches nothing is a worse lie than no
    // button.
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Load more" })).toBeNull(),
    );
  });

  it("should keep the rows already shown when a page fails, and keep the control", async () => {
    listActionMock.mockResolvedValue({
      ok: false,
      status: 503,
      errorCode: "UZ-LIBRARY-006",
      error: "the datastore did not answer",
    });
    renderList([entry("e1", "github-pr-reviewer")], CURSOR);

    fireEvent.click(screen.getByRole("button", { name: "Load more" }));

    await waitFor(() => expect(screen.getByText("github-pr-reviewer")).toBeTruthy());
    expect(renderedNames()).toEqual(["github-pr-reviewer"]);
    expect(await screen.findByRole("button", { name: "Load more" })).toBeTruthy();
  });
});

describe("a removal invalidates the pages after the first", () => {
  const CURSOR = "cursor-page-2";

  it("should drop the appended pages so a shifted row cannot render twice", async () => {
    // The collection is keyset-paged: removing a row shifts the rest up across
    // the page boundary, so the refreshed first page can re-contain a row the
    // client already appended. Keeping both renders it twice under one key.
    listActionMock.mockResolvedValue({
      ok: true,
      data: { items: [entry("e2", "incident-responder")], total: null, next_cursor: null },
    });
    removeActionMock.mockResolvedValue({ ok: true, data: undefined });
    renderList([entry("e1", "github-pr-reviewer")], CURSOR);

    fireEvent.click(screen.getByRole("button", { name: "Load more" }));
    await waitFor(() => expect(screen.getByText("incident-responder")).toBeTruthy());
    expect(renderedNames()).toEqual(["github-pr-reviewer", "incident-responder"]);

    fireEvent.click(screen.getAllByRole("button", { name: "Remove" })[0]!);
    await waitFor(() => screen.getByText(/from this workspace\?/));
    const confirms = screen.getAllByRole("button", { name: "Remove" });
    fireEvent.click(confirms[confirms.length - 1]!);

    await waitFor(() => expect(refreshMock).toHaveBeenCalled());
    // Only the server's own first page survives; the appended page is gone.
    await waitFor(() => expect(screen.queryByText("incident-responder")).toBeNull());
    expect(new Set(renderedNames()).size).toBe(renderedNames().length);
  });

  it("should put Load more back on the server's cursor, not the stale one", async () => {
    listActionMock.mockResolvedValue({
      ok: true,
      data: { items: [entry("e2", "incident-responder")], total: null, next_cursor: null },
    });
    removeActionMock.mockResolvedValue({ ok: true, data: undefined });
    renderList([entry("e1", "github-pr-reviewer")], CURSOR);

    // Exhaust the walk, so the control is gone.
    fireEvent.click(screen.getByRole("button", { name: "Load more" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Load more" })).toBeNull());
    // The row actions are disabled while the page transition is in flight, and
    // a click on a disabled button is silently dropped.
    await waitFor(() =>
      expect(
        (screen.getAllByRole("button", { name: "Remove" })[0] as HTMLButtonElement).disabled,
      ).toBe(false),
    );

    fireEvent.click(screen.getAllByRole("button", { name: "Remove" })[0]!);
    await waitFor(() => screen.getByText(/from this workspace\?/));
    const confirms = screen.getAllByRole("button", { name: "Remove" });
    fireEvent.click(confirms[confirms.length - 1]!);
    await waitFor(() => expect(refreshMock).toHaveBeenCalled());

    // Back to the cursor the server handed down, so the rest stays reachable
    // instead of being stranded behind an exhausted walk.
    expect(await screen.findByRole("button", { name: "Load more" })).toBeTruthy();
  });
});
