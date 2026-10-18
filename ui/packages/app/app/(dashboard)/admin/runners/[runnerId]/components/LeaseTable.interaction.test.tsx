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
