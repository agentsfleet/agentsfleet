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

const { credentialMock, removeMock } = vi.hoisted(() => ({
  credentialMock: vi.fn(),
  removeMock: vi.fn(),
}));
vi.mock("@/lib/auth/credential", () => ({ credential: credentialMock }));
vi.mock("@/lib/api/fleet-library", () => ({ removeWorkspaceLibraryEntry: removeMock }));

import { ApiError } from "@/lib/api/errors";
import { removeLibraryEntryAction } from "./actions";

const WORKSPACE = "ws_1";
const ENTRY = "0199c5a0-0000-7000-8000-00000000000a";
const TOKEN = "a-session-token";

beforeEach(() => {
  credentialMock.mockReset();
  removeMock.mockReset();
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
