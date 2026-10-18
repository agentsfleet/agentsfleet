import { Profiler } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { RecentFleets, RecentFleetsProvider, RecentFleetVisit, useRememberFleets } from "@/components/layout/RecentFleets";

import { agentDisplayName } from "@/lib/fleets/agent-label";

afterEach(cleanup);
const FLEETS = [{id: "first", name: "First fleet"}, {id: "second", name: "Second fleet"}];
function Remember({fleets = FLEETS, workspace = "one"}) { useRememberFleets(workspace, fleets); return null; }

it("keeps visits first, updates names, and isolates workspace history", () => {
  const navigate = vi.fn();
  const sidebar = (workspace: string | null, pathname = "/w/one/fleets/first") =>
    <RecentFleets workspaceId={workspace} pathname={pathname} onNavigate={navigate} />;
  const view = render(<RecentFleetsProvider><Remember />{sidebar("one")}</RecentFleetsProvider>);
  expect(screen.getAllByRole("link").map((link) => link.getAttribute("aria-label"))).toEqual(FLEETS.map((fleet) => agentDisplayName(fleet.id)));
  view.rerender(<RecentFleetsProvider><Remember /><RecentFleetVisit workspace="one" id="second" name="Visited fleet" />{sidebar("one", "/w/one/fleets/second/activity")}</RecentFleetsProvider>);
  expect(screen.getAllByRole("link")[0]?.getAttribute("aria-label")).toBe(agentDisplayName("second"));
  expect(screen.getByText("Visited fleet")).toBeTruthy();
  fireEvent.click(screen.getByRole("link", {name: agentDisplayName("second")}));
  expect(navigate).toHaveBeenCalledOnce();
  view.rerender(<RecentFleetsProvider><Remember fleets={[{id: "second", name: "Renamed fleet"}]} />{sidebar("one")}</RecentFleetsProvider>);
  expect(screen.getAllByRole("link").map((link) => link.getAttribute("aria-label"))).toEqual([agentDisplayName("second"), agentDisplayName("first")]);
  expect(screen.getByText("Renamed fleet")).toBeTruthy();
  view.rerender(<RecentFleetsProvider>{sidebar("other")}</RecentFleetsProvider>);
  expect(screen.queryByRole("link")).toBeNull();
  expect(screen.getByText("Fleets you open appear here.")).toBeTruthy();
  view.rerender(<RecentFleetsProvider>{sidebar(null)}</RecentFleetsProvider>);
  expect(screen.queryByRole("link")).toBeNull();
});

it("bounds recent history and does not rerender unchanged records", () => {
  const committed = vi.fn();
  const fleets = Array.from({length: 15}, (_, index) => ({id: String(index), name: `Fleet ${index}`}));
  const sidebar = <Profiler id="recents" onRender={committed}><RecentFleets workspaceId="one" pathname="/" onNavigate={() => {}} /></Profiler>;
  const view = render(<RecentFleetsProvider><Remember fleets={fleets} />{sidebar}</RecentFleetsProvider>);
  expect(screen.getAllByRole("link")).toHaveLength(12);
  expect(screen.queryByRole("link", {name: agentDisplayName("12")})).toBeNull();
  const count = committed.mock.calls.length;
  view.rerender(<RecentFleetsProvider><Remember fleets={fleets.map((fleet) => ({...fleet}))} />{sidebar}</RecentFleetsProvider>);
  expect(committed).toHaveBeenCalledTimes(count);
  view.unmount();
  render(<RecentFleetsProvider>{sidebar}</RecentFleetsProvider>);
  expect(screen.queryByRole("link")).toBeNull();
});

it("permits rendering consumers before a history provider mounts", () => {
  render(<><Remember /><RecentFleetVisit workspace="one" id="first" name="Private" /><RecentFleets workspaceId="one" pathname="/" onNavigate={() => {}} /></>);
  expect(screen.queryByText("Private")).toBeNull();
});
