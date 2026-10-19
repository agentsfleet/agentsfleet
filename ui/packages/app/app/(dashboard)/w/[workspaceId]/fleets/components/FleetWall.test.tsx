import React from "react";
import { afterEach, expect, it, vi } from "vitest";
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
  return { id: "f1", name: "alpha", description: null, status: "active", created_at: "1970-01-01T00:00:00Z", updated_at: "1970-01-01T00:00:00Z", events_processed: 0n, budget_used_nanos: 0n, ...over };
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

/*
 * The badge is queried by ROLE and CONTENT, never by `aria-label`.
 *
 * It used to carry `aria-label={text}` on a bare <span>, and these tests
 * asserted that label with `getByLabelText`. Both were wrong in the same way:
 * ARIA gives a plain span no role for a name to attach to, so the label never
 * reached a screen reader — and `getByLabelText` reads the attribute rather
 * than the computed accessibility tree, so the tests passed anyway.
 *
 * `<output>` carries an implicit `role="status"`. A status region has no
 * accessible NAME — it is not a name-from-content role — it announces its
 * CONTENT when that content changes, which is exactly the behaviour the wall
 * wants. So the assertion is on text content, and it now fails if the live
 * region stops existing.
 */
it("FleetWall > omits the fleet section when the loaded list is empty and complete", () => {
  const { container } = renderWall([]);
  expect(screen.queryByText("Your fleets")).toBeNull();
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.queryByRole("button", { name: /load more/i })).toBeNull();
  expect(container.childElementCount).toBe(0);
});

it("FleetWall > keeps pagination and its failure visible when an empty page has a continuation", async () => {
  listFleetsAction.mockResolvedValue({ ok: false, error: "boom", errorCode: "UZ-X" });
  renderWall([], "cur_1");
  await userEvent.setup().click(screen.getByRole("button", { name: /load more/i }));
  await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/load more fleets/i));
  expect(screen.queryByText("Your fleets")).toBeNull();
  expect(screen.getByRole("button", { name: /load more/i })).toBeTruthy();
});

it("FleetWall > renders one tile per fleet and a live count for active fleets", () => {
  renderWall([fleet(), fleet({ id: "f2", name: "beta", status: "stopped" })]);
  expect(screen.getAllByTestId("tile")).toHaveLength(2);
  expect(screen.getByRole("status").textContent).toContain("1 live");
  expect(screen.getByText("Your fleets")).toBeTruthy();
});

it("FleetWall > refuses to claim live before the stream is connected and greeted", () => {
  // The old eyebrow counted ACTIVE fleets, so it read "1 live" while the
  // EventSource was still opening — the page asserting liveness it had no
  // evidence for. Connection state, not fleet status, decides this now.
  streamState = { ...liveStream(), connectionStatus: "connecting", helloReceived: false };
  renderWall([fleet()]);
  expect(screen.getByRole("status").textContent).not.toContain("1 live");
  expect(screen.getByRole("status").textContent).toContain("connecting…");
});

it("FleetWall > says reconnecting when the stream dropped, rather than going quiet", () => {
  streamState = { ...liveStream(), connectionStatus: "reconnecting" };
  renderWall([fleet()]);
  expect(screen.getByRole("status").textContent).toContain("reconnecting…");
});

it("FleetWall > says offline when the stream has given up, rather than going quiet", () => {
  streamState = { ...liveStream(), connectionStatus: "offline" };
  renderWall([fleet()]);
  expect(screen.getByRole("status").textContent).toContain("offline");
  expect(screen.getByRole("status").textContent).not.toContain("1 live");
});

it("FleetWall > hides the live counter when no loaded fleet is active", () => {
  renderWall([fleet({ status: "stopped" }), fleet({ id: "f2", name: "beta", status: "killed" })]);
  // A wall of parked/killed fleets shows no "N live" eyebrow — a zero count
  // is silence, not "0 live".
  // No badge at all, so no live region to announce anything.
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.getAllByTestId("tile")).toHaveLength(2);
});

it("FleetWall > appends the next page on Load more", async () => {
  listFleetsAction.mockResolvedValue({ ok: true, data: { items: [fleet({ id: "f2", name: "beta" })], next_cursor: null } });
  const user = userEvent.setup();
  renderWall([fleet()], "cur_1");
  await user.click(screen.getByRole("button", { name: /load more/i }));
  await waitFor(() => expect(screen.getAllByTestId("tile")).toHaveLength(2));
  expect(listFleetsAction).toHaveBeenCalledWith("ws_1", { starting_after: "cur_1" });
});

it("FleetWall > surfaces an error when Load more fails", async () => {
  listFleetsAction.mockResolvedValue({ ok: false, error: "boom", errorCode: "UZ-X" });
  const user = userEvent.setup();
  renderWall([fleet()], "cur_1");
  await user.click(screen.getByRole("button", { name: /load more/i }));
  await waitFor(() => expect(screen.getByText(/load more fleets/i)).toBeTruthy());
});
