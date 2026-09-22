/**
 * The owned-collection client issues the paths the route table serves.
 *
 * The gallery and this collection are one module and one page apart, and the
 * install flow resolves `--library <id>` against the gallery — so a client
 * that reached the wrong one would list rows a workspace cannot remove, or
 * offer a removal that answers 204 for every platform entry in the catalogue.
 */
import { describe, expect, it, vi, beforeEach } from "vitest";

const { requestMock } = vi.hoisted(() => ({ requestMock: vi.fn() }));
vi.mock("@/lib/api/client", () => ({ request: requestMock }));

import {
  listWorkspaceLibraryEntries,
  removeWorkspaceLibraryEntry,
  listWorkspaceFleetLibrary,
} from "@/lib/api/fleet-library";

beforeEach(() => requestMock.mockReset());

describe("the owned-entries client", () => {
  it("reads /library-entries, not the gallery", async () => {
    requestMock.mockResolvedValue({ items: [], total: null, next_cursor: null });

    await listWorkspaceLibraryEntries("ws_1", "token");

    const path = requestMock.mock.calls[0]?.[0] as string;
    expect(path.startsWith("/v1/workspaces/ws_1/library-entries?")).toBe(true);
    expect(path).not.toContain("fleet-libraries");
  });

  it("asks for the largest window one round-trip buys", async () => {
    // 100 is the server's bound; above it the read earns UZ-LIBRARY-003.
    requestMock.mockResolvedValue({ items: [], total: null, next_cursor: null });

    await listWorkspaceLibraryEntries("ws_1", "token");

    expect(requestMock.mock.calls[0]?.[0]).toContain("limit=100");
  });

  it("sends this collection's own cursor, under the shared parameter name", async () => {
    requestMock.mockResolvedValue({ items: [], total: null, next_cursor: null });

    await listWorkspaceLibraryEntries("ws_1", "token", "cursor-abc");

    expect(requestMock.mock.calls[0]?.[0]).toContain("starting_after=cursor-abc");
  });

  it("removes one entry by DELETE on the single-entry path", async () => {
    requestMock.mockResolvedValue(undefined);

    await removeWorkspaceLibraryEntry("ws_1", "e1", "token");

    expect(requestMock.mock.calls[0]?.[0]).toBe("/v1/workspaces/ws_1/library-entries/e1");
    expect(requestMock.mock.calls[0]?.[1]).toEqual({ method: "DELETE" });
  });

  it("encodes an identifier that would otherwise escape its segment", async () => {
    requestMock.mockResolvedValue(undefined);

    await removeWorkspaceLibraryEntry("ws_1", "../fleets", "token");

    expect(requestMock.mock.calls[0]?.[0]).toBe("/v1/workspaces/ws_1/library-entries/..%2Ffleets");
  });

  it("leaves the gallery reading the gallery", async () => {
    // The preserved behaviour: `install --library <id>` resolves here, so the
    // shipped path must not move because a second collection arrived.
    requestMock.mockResolvedValue({ items: [], total: null, next_cursor: null });

    await listWorkspaceFleetLibrary("ws_1", "token");

    expect(requestMock.mock.calls[0]?.[0]).toContain("/v1/workspaces/ws_1/fleet-libraries?");
  });
});
