import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { lease } from "@/tests/helpers/runner-leases";
import { response } from "@/tests/helpers/draft-fixtures";
import { agentDisplayName } from "@/lib/fleets/agent-label";
import { LeaseTable } from "./LeaseTable";

beforeEach(() => vi.stubGlobal("fetch", vi.fn(async () => response({}, 404))));
afterEach(() => {cleanup(); vi.unstubAllGlobals();});
it("puts inspection beneath Fleet and uses the shared Status and Created order", () => {
  render(<LeaseTable initial={{items: [lease({id: "lease-1"})], total: 1, next_cursor: null}} pageSize={25} />, {wrapper: TooltipProvider});
  expect(screen.getAllByRole("columnheader").map((header) => header.textContent)).toEqual([
    "Fleet", "Workspace", "Work", "Created", "Duration", "Status",
  ]);
  const row = screen.getAllByRole("row")[1];
  if (!row) throw new Error("Expected the recorded lease");
  const fleet = within(row).getAllByRole("cell")[0];
  if (!fleet) throw new Error("Expected the Fleet cell");
  const action = within(fleet).getByRole("button", {name: `View lease for ${agentDisplayName("fleet-1")}`});
  expect(within(fleet).getByText(agentDisplayName("fleet-1"))).toBeTruthy();
  fireEvent.click(row);
  expect(screen.queryByRole("dialog")).toBeNull();
  fireEvent.click(action);
  expect(screen.getByRole("dialog", {name: "Review lease"})).toBeTruthy();
});

it("retains coherent work history and a callsign when the fleet and event are gone", () => {
  const retained = lease({id: "retained", fleet_name: "old-name", destination: "fleet_deleted",
    event_type: null, actor: null, outcome: "unknown"});
  render(<LeaseTable initial={{items: [retained], total: 1, next_cursor: null}} pageSize={25} />, {wrapper: TooltipProvider});
  const row = screen.getAllByRole("row")[1];
  if (!row) throw new Error("Expected retained history");
  expect(within(row).getByText(agentDisplayName(retained.fleet_id))).toBeTruthy();
  expect(within(row).getByText("Event history unavailable")).toBeTruthy();
  expect(within(row).queryByText(/^by/)).toBeNull();
  fireEvent.click(within(row).getByRole("button", {name: /View lease/}));
  const dialog = screen.getByRole("dialog", {name: "Review lease"});
  expect(within(dialog).getByText("This fleet was deleted. Its lease history is retained.")).toBeTruthy();
  expect(within(dialog).queryByRole("link", {name: /Open fleet/})).toBeNull();
});

it.each(["fleet_deleted", "not_permitted"] as const)("keeps an unavailable %s workspace readable without a broken link", (destination) => {
  const retained = lease({destination});
  render(<LeaseTable initial={{items: [retained], total: 1, next_cursor: null}} pageSize={25} />, {wrapper: TooltipProvider});
  const row = screen.getAllByRole("row")[1];
  if (!row) throw new Error("Expected retained history");
  expect(within(row).getByTitle(retained.workspace_id).textContent).toContain(retained.workspace_id.slice(0, 8));
  expect(within(row).queryByRole("link")).toBeNull();
});
