/**
 * Walking the workspace library beyond its first page, and what a removal does
 * to the pages already walked.
 *
 * The collection is keyset-paged, so the client holds appended pages the server
 * does not know about. That split state is the subject here: disclosing that
 * more exists, following the cursor, and discarding what a removal invalidated.
 * `WorkspaceLibraryList.test.tsx` covers rendering, sorting and the removal
 * confirmation on a single-page list.
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

/**
 * The entry names the table renders, top row first.
 *
 * `queryAllByRole` rather than `getAllByRole`: a list that has emptied renders
 * an empty state and no row at all, and the getter throws where every caller
 * here wants the empty answer.
 */
function renderedNames(): string[] {
  return screen
    .queryAllByRole("row")
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

/**
 * Click a row's Remove once that row action is live, then wait for the question.
 *
 * The row actions are disabled while a page transition is in flight, and a
 * click on a disabled button is silently dropped rather than queued. Waiting on
 * the row's text is not enough: the row renders before the transition settles,
 * so a bare click passes on a quiet machine and times out at the confirmation
 * on a loaded one.
 */
async function removeRow(index = 0) {
  const action = screen.getAllByRole("button", { name: "Remove" })[index] as HTMLButtonElement;
  expect(action.disabled).toBe(false);
  fireEvent.click(action);
  await waitFor(() => screen.getByText(/from this workspace\?/));
  const confirms = screen.getAllByRole("button", { name: "Remove" });
  fireEvent.click(confirms[confirms.length - 1]!);
}

afterEach(() => {
  cleanup();
  refreshMock.mockReset();
  removeActionMock.mockReset();
  listActionMock.mockReset();
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

describe("a page fetch does not shut the row actions", () => {
  const CURSOR = "cursor-page-2";

  it("should keep Remove clickable while Load more is in flight", async () => {
    // The defect this pins: `pending` is one transition shared by Load more
    // and the removal, and the row action read it. Appending a page therefore
    // disabled a button that sends nothing — opening the question is local
    // state — and a click landing in that window is not queued or replayed,
    // it is dropped. A person clicked Remove, saw nothing, and had to find
    // out that a second click works.
    let releasePage: (value: unknown) => void = () => {};
    listActionMock.mockReturnValue(
      new Promise((resolve) => {
        releasePage = resolve;
      }),
    );
    renderList([entry("e1", "github-pr-reviewer")], CURSOR);

    fireEvent.click(screen.getByRole("button", { name: "Load more" }));
    // Mid-flight: the page has not arrived, so the control reads its pending
    // label. That IS the assertion that a transition is running — the name
    // changes with `pending`, so finding it proves the state this case needs.
    await waitFor(() => expect(screen.getByRole("button", { name: "Loading…" })).toBeTruthy());

    const action = screen.getAllByRole("button", { name: "Remove" })[0] as HTMLButtonElement;
    expect(action.disabled).toBe(false);
    fireEvent.click(action);

    // The click was taken, not dropped: the question is open, during the fetch.
    await waitFor(() => expect(screen.getByText(/from this workspace\?/)).toBeTruthy());

    releasePage({
      ok: true,
      data: { items: [entry("e2", "incident-responder")], total: null, next_cursor: null },
    });
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

    await removeRow();

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

    await removeRow();
    await waitFor(() => expect(refreshMock).toHaveBeenCalled());

    // Back to the cursor the server handed down, so the rest stays reachable
    // instead of being stranded behind an exhausted walk.
    expect(await screen.findByRole("button", { name: "Load more" })).toBeTruthy();
  });
});
