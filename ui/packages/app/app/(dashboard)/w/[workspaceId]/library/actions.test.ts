/**
 * The removal action's three answers.
 *
 * It is four lines long and every one of them is a branch a page renders
 * differently: a token it could not resolve, a refusal the daemon sent, and
 * the success that refreshes the list. `withToken` owns the shape; what this
 * file pins is that the action reaches the right endpoint with the caller's
 * token and does not swallow either failure into a success.
 */
import { describe, expect, it, vi, beforeEach } from "vitest";

const { credentialMock, removeMock, listMock } = vi.hoisted(() => ({
  credentialMock: vi.fn(),
  removeMock: vi.fn(),
  listMock: vi.fn(),
}));
vi.mock("@/lib/auth/credential", () => ({ credential: credentialMock }));
vi.mock("@/lib/api/fleet-library", () => ({
  removeWorkspaceLibraryEntry: removeMock,
  listWorkspaceLibraryEntries: listMock,
}));

import { ApiError } from "@/lib/api/errors";
import { listLibraryEntriesAction, removeLibraryEntryAction } from "./actions";

const WORKSPACE = "ws_1";
const ENTRY = "0199c5a0-0000-7000-8000-00000000000a";
const TOKEN = "a-session-token";

const CURSOR = "cursor-page-2";

beforeEach(() => {
  credentialMock.mockReset();
  removeMock.mockReset();
  listMock.mockReset();
});

describe("removeLibraryEntryAction", () => {
  it("should remove the named entry with the caller's token when authenticated", async () => {
    credentialMock.mockResolvedValue(TOKEN);
    removeMock.mockResolvedValue(undefined);

    const result = await removeLibraryEntryAction(WORKSPACE, ENTRY);

    expect(result.ok).toBe(true);
    // The workspace and the entry both reach the client, in that order, with
    // the token behind them: a swapped pair would remove the wrong row and a
    // dropped token would answer 401 from the daemon instead of here.
    expect(removeMock).toHaveBeenCalledWith(WORKSPACE, ENTRY, TOKEN);
  });

  it("should refuse without calling the endpoint when no token resolves", async () => {
    credentialMock.mockResolvedValue(null);

    const result = await removeLibraryEntryAction(WORKSPACE, ENTRY);

    expect(result).toMatchObject({ ok: false, status: 401 });
    // The point of the branch: an unauthenticated caller never reaches the
    // network, so a missing session cannot read as a removal that did nothing.
    expect(removeMock).not.toHaveBeenCalled();
  });

  it("should carry the daemon's status and code when the removal is refused", async () => {
    credentialMock.mockResolvedValue(TOKEN);
    removeMock.mockRejectedValue(
      new ApiError("the datastore would not answer", 500, "UZ-LIBRARY-006"),
    );

    const result = await removeLibraryEntryAction(WORKSPACE, ENTRY);

    // Type, message and structured fields — the page renders the sentence and
    // branches on the code, so losing either turns a refusal into a blank row.
    expect(result).toMatchObject({
      ok: false,
      status: 500,
      errorCode: "UZ-LIBRARY-006",
      error: "the datastore would not answer",
    });
  });
});

/**
 * The later-page read behind the list's Load more. The first page is rendered
 * by the page itself, so this action only ever runs with a cursor in hand —
 * and it has to pass that cursor through, because an action that silently
 * re-read page one would loop the control forever on the same rows.
 */
describe("listLibraryEntriesAction", () => {
  const page = { items: [], total: null, next_cursor: null };

  it("should read the next page with the caller's token and the given cursor", async () => {
    credentialMock.mockResolvedValue(TOKEN);
    listMock.mockResolvedValue(page);

    const result = await listLibraryEntriesAction(WORKSPACE, CURSOR);

    expect(result).toMatchObject({ ok: true, data: page });
    // Workspace, token, cursor — in that order. A dropped cursor re-reads the
    // first page, which renders the rows already shown and never advances.
    expect(listMock).toHaveBeenCalledWith(WORKSPACE, TOKEN, CURSOR);
  });

  it("should refuse without calling the endpoint when no token resolves", async () => {
    credentialMock.mockResolvedValue(null);

    const result = await listLibraryEntriesAction(WORKSPACE, CURSOR);

    expect(result).toMatchObject({ ok: false, status: 401 });
    expect(listMock).not.toHaveBeenCalled();
  });

  it("should carry the daemon's status and code when the page is refused", async () => {
    credentialMock.mockResolvedValue(TOKEN);
    listMock.mockRejectedValue(
      new ApiError("that cursor was issued for another walk", 400, "UZ-LIBRARY-002"),
    );

    const result = await listLibraryEntriesAction(WORKSPACE, CURSOR);

    // The list renders this sentence beside the rows it already has, so the
    // status and the code both have to survive the action.
    expect(result).toMatchObject({
      ok: false,
      status: 400,
      errorCode: "UZ-LIBRARY-002",
      error: "that cursor was issued for another walk",
    });
  });
});
