import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Fleet } from "@/lib/api/fleets";

vi.mock("next/link", () => ({
  default: ({ href, children }: React.PropsWithChildren<{ href: string }>) =>
    React.createElement("a", { href }, children),
}));
// Stub the tile so the wall test never touches the stream registry.
vi.mock("./FleetTile", () => ({
  default: ({ fleet }: { fleet: Fleet }) =>
    React.createElement("div", { "data-testid": "tile", "data-status": fleet.status }, fleet.name),
}));
// The badge reports the STREAM's state, not the count of active fleets, so a
// wall test has to say which stream state it is describing. Connected-and-
// greeted is the default; `streamState` is reassigned by the test that checks
// the wall refuses to claim "live" before the stream says so.
let streamState = {
  connectionStatus: "live",
  helloReceived: true,
  catchingUp: false,
};
vi.mock("@/components/domain/useWorkspaceStream", () => ({
  WorkspaceStreamProvider: ({ children }: React.PropsWithChildren) =>
    React.createElement(React.Fragment, null, children),
  useWorkspaceStream: () => streamState,
}));
const listFleetsAction = vi.fn();
vi.mock("../actions", () => ({ listFleetsAction: (...a: unknown[]) => listFleetsAction(...a) }));

import FleetWall from "./FleetWall";

function liveStream() {
  return { connectionStatus: "live", helloReceived: true, catchingUp: false };
}

function fleet(over: Partial<Fleet> = {}): Fleet {
  return { id: "f1", name: "alpha", status: "active", created_at: 0, updated_at: 0, ...over };
}

function renderWall(fleets: Fleet[], cursor: string | null = null) {
  return render(
    React.createElement(FleetWall, {
      workspaceId: "ws_1",
      initialFleets: fleets,
      initialCursor: cursor,
    }),
  );
}

afterEach(() => {
  cleanup();
  listFleetsAction.mockReset();
  streamState = liveStream();
});

describe("FleetWall", () => {
  it("renders one tile per fleet and a live count for active fleets", () => {
    renderWall([fleet(), fleet({ id: "f2", name: "beta", status: "stopped" })]);
    expect(screen.getAllByTestId("tile")).toHaveLength(2);
    expect(screen.getByLabelText("1 live")).toBeTruthy();
  });

  it("refuses to claim live before the stream is connected and greeted", () => {
    // The old eyebrow counted ACTIVE fleets, so it read "1 live" while the
    // EventSource was still opening — the page asserting liveness it had no
    // evidence for. Connection state, not fleet status, decides this now.
    streamState = { ...liveStream(), connectionStatus: "connecting", helloReceived: false };
    renderWall([fleet()]);
    expect(screen.queryByLabelText("1 live")).toBeNull();
    expect(screen.getByLabelText("connecting…")).toBeTruthy();
  });

  it("says reconnecting when the stream dropped, rather than going quiet", () => {
    streamState = { ...liveStream(), connectionStatus: "reconnecting" };
    renderWall([fleet()]);
    expect(screen.getByLabelText("reconnecting…")).toBeTruthy();
  });

  it("says offline when the stream has given up, rather than going quiet", () => {
    streamState = { ...liveStream(), connectionStatus: "offline" };
    renderWall([fleet()]);
    expect(screen.getByLabelText("offline")).toBeTruthy();
    expect(screen.queryByLabelText("1 live")).toBeNull();
  });

  it("hides the live counter when no loaded fleet is active", () => {
    renderWall([fleet({ status: "stopped" }), fleet({ id: "f2", name: "beta", status: "killed" })]);
    // A wall of parked/killed fleets shows no "N live" eyebrow — a zero count
    // is silence, not "0 live".
    expect(screen.queryByLabelText(/live$/)).toBeNull();
    expect(screen.getAllByTestId("tile")).toHaveLength(2);
  });

  it("test_wall_header_has_no_search", () => {
    renderWall([fleet(), fleet({ id: "f2", name: "beta" })]);
    // The wall is tiles, live count, and Install fleet — no client-side filter
    // box. Every loaded fleet renders unfiltered.
    expect(screen.queryByRole("searchbox")).toBeNull();
    expect(screen.queryByLabelText("Search fleets")).toBeNull();
    expect(screen.getAllByTestId("tile")).toHaveLength(2);
    expect(screen.getByText(/install fleet/i)).toBeTruthy();
  });

  it("appends the next page on Load more", async () => {
    listFleetsAction.mockResolvedValue({ ok: true, data: { items: [fleet({ id: "f2", name: "beta" })], next_cursor: null } });
    const user = userEvent.setup();
    renderWall([fleet()], "cur_1");
    await user.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() => expect(screen.getAllByTestId("tile")).toHaveLength(2));
    expect(listFleetsAction).toHaveBeenCalledWith("ws_1", { starting_after: "cur_1" });
  });

  it("surfaces an error when Load more fails", async () => {
    listFleetsAction.mockResolvedValue({ ok: false, error: "boom", errorCode: "UZ-X" });
    const user = userEvent.setup();
    renderWall([fleet()], "cur_1");
    await user.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() => expect(screen.getByText(/load more fleets/i)).toBeTruthy());
  });
});
