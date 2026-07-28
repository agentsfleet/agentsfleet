import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The install screen's only server-side read. The first gallery page arrives
// with the server render; this action exists for load-more, which is a client
// gesture and so cannot call the bearer-authed endpoint directly. The token is
// minted inside `withToken` and never reaches the browser — that is the whole
// reason the wrapper exists, so both module boundaries are mocked and the
// token's provenance is asserted rather than assumed.
const { withTokenMock, listWorkspaceFleetLibraryMock } = vi.hoisted(() => ({
  withTokenMock: vi.fn(),
  listWorkspaceFleetLibraryMock: vi.fn(),
}));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/fleet-library", () => ({
  listWorkspaceFleetLibrary: listWorkspaceFleetLibraryMock,
}));

import { readFleetLibraryPageAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/new/actions";

const PAGE = { items: [], next_cursor: "cur-2", total: null };

beforeEach(() => {
  vi.clearAllMocks();
  // Faithful to the real withToken: forward a resolved token, and normalise a
  // throw into { ok: false } rather than letting it escape the action.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => {
    try {
      return { ok: true, data: await fn("tok") };
    } catch (e) {
      return { ok: false, error: e instanceof Error ? e.message : String(e) };
    }
  });
});
afterEach(() => vi.resetAllMocks());

describe("readFleetLibraryPageAction", () => {
  it("reads the FIRST page when the caller names no cursor", async () => {
    // The default is what the recovery path sends: a gallery whose
    // server-render read failed has no cursor in hand and must re-read page
    // one, so `null` has to mean "start over" rather than "no argument".
    listWorkspaceFleetLibraryMock.mockResolvedValue(PAGE);

    const result = await readFleetLibraryPageAction("ws_1");

    expect(result).toEqual({ ok: true, data: PAGE });
    expect(listWorkspaceFleetLibraryMock).toHaveBeenCalledWith("ws_1", "tok", null);
  });

  it("resumes from the cursor it is given, with the token injected by withToken", async () => {
    listWorkspaceFleetLibraryMock.mockResolvedValue(PAGE);

    const result = await readFleetLibraryPageAction("ws_1", "cur-2");

    expect(result).toEqual({ ok: true, data: PAGE });
    expect(withTokenMock).toHaveBeenCalledTimes(1);
    // The cursor is the caller's; the token is not — it is minted server-side.
    expect(listWorkspaceFleetLibraryMock).toHaveBeenCalledWith("ws_1", "tok", "cur-2");
  });

  it("returns a typed failure rather than throwing across the boundary", async () => {
    // A Server Action cannot throw with custom fields intact, so the gallery
    // branches on `ok`. An escaping rejection would reach a route with no
    // error boundary instead of the typed failure the picker renders.
    listWorkspaceFleetLibraryMock.mockRejectedValue(new Error("catalog down"));

    const result = await readFleetLibraryPageAction("ws_1", "cur-2");

    expect(result).toEqual({ ok: false, error: "catalog down" });
  });
});
