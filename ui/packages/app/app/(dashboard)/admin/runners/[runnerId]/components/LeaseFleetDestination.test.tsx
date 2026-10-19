import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { FLEET_ID, WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { LeaseFleetDestination } from "./LeaseFleetDestination";

afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it("opens an explicitly authorized fleet without a second request", () => {
  const fetchMock = vi.fn(); vi.stubGlobal("fetch", fetchMock);
  render(<LeaseFleetDestination workspaceId={WORKSPACE} fleetId={FLEET_ID} destination="available" />);
  expect(screen.getByRole("link", {name: /Open fleet/}).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/${FLEET_ID}`);
  expect(fetchMock).not.toHaveBeenCalled();
});

it.each([
  ["fleet_deleted", "This fleet was deleted. Its lease history is retained."],
  ["not_permitted", "This fleet belongs to another account."],
] as const)("keeps the lease readable without offering a broken destination: %s", (destination, message) => {
  render(<LeaseFleetDestination workspaceId={WORKSPACE} fleetId={FLEET_ID} destination={destination} />);
  expect(screen.getByText(message)).toBeTruthy();
  expect(screen.queryByRole("link")).toBeNull();
  expect(screen.queryByRole("button")).toBeNull();
});

it("does not retain a previous authorization across a destination change", () => {
  const view = render(<LeaseFleetDestination workspaceId={WORKSPACE} fleetId={FLEET_ID} destination="available" />);
  view.rerender(<LeaseFleetDestination workspaceId="other" fleetId={FLEET_ID} destination="not_permitted" />);
  expect(screen.queryByRole("link")).toBeNull();
  view.rerender(<LeaseFleetDestination workspaceId="other" fleetId={FLEET_ID} destination="available" />);
  expect(screen.getByRole("link").getAttribute("href")).toBe(`/w/other/fleets/${FLEET_ID}`);
});
