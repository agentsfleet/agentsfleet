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

const { refreshMock, removeActionMock } = vi.hoisted(() => ({
  refreshMock: vi.fn(),
  removeActionMock: vi.fn(),
}));
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: refreshMock, push: vi.fn() }),
}));
vi.mock("../actions", () => ({ removeLibraryEntryAction: removeActionMock }));

import WorkspaceLibraryList from "./WorkspaceLibraryList";

const CREATED_MS = Date.UTC(2026, 3, 30, 10, 30, 0);

function entry(id: string, name: string): WorkspaceLibraryEntry {
  return {
    id,
    name,
    description: "Reviews pull requests.",
    source_kind: "github",
    source_ref: "acme/reviewer",
    content_hash: `hash-${id}`,
    created_at: CREATED_MS,
  };
}

function renderList(entries: WorkspaceLibraryEntry[]) {
  return render(
    React.createElement(
      TooltipProvider,
      null,
      React.createElement(WorkspaceLibraryList, { workspaceId: "ws_1", entries }),
    ),
  );
}

afterEach(() => {
  cleanup();
  refreshMock.mockReset();
  removeActionMock.mockReset();
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
});
