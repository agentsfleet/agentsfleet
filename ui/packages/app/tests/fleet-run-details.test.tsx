import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { FleetRunDetails } from "@/components/domain/FleetRunDetails";
import { FleetScopeProvider } from "@/components/domain/FleetScope";
import { SCHEDULE_EVENT } from "./helpers/schedule-activity";

const read = vi.hoisted(() => vi.fn());
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({getFleetEventAction: read}));

beforeEach(() => vi.resetAllMocks());
afterEach(cleanup);

it("offers no native read without fleet ownership or before an event record exists", () => {
  const view = render(<FleetRunDetails eventId={SCHEDULE_EVENT.event_id} status="processed" />);
  expect(screen.queryByRole("button", {name: "Run details"})).toBeNull();
  for (const status of ["queued", "optimistic", "recorded", "future_status", ""]) {
    view.rerender(<FleetScopeProvider workspaceId={SCHEDULE_EVENT.workspace_id} fleetId={SCHEDULE_EVENT.fleet_id}>
      <FleetRunDetails eventId={SCHEDULE_EVENT.event_id} status={status} />
    </FleetScopeProvider>);
    expect(screen.queryByRole("button", {name: "Run details"})).toBeNull();
  }
  expect(read).not.toHaveBeenCalled();
});

it("loads the exact native run on demand and refreshes the open record after completion", async () => {
  const pending = Promise.withResolvers<{ok: true; data: typeof SCHEDULE_EVENT}>();
  read.mockReturnValue(pending.promise);
  const view = render(<TooltipProvider><FleetScopeProvider workspaceId={SCHEDULE_EVENT.workspace_id} fleetId={SCHEDULE_EVENT.fleet_id}>
    <FleetRunDetails eventId={SCHEDULE_EVENT.event_id} status="received" />
  </FleetScopeProvider></TooltipProvider>);
  expect(read).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", {name: "Run details"}));
  expect(await screen.findByText("Loading run details…")).toBeTruthy();
  expect(screen.queryByText("No result recorded")).toBeNull();
  expect(read).toHaveBeenCalledWith(SCHEDULE_EVENT.workspace_id, SCHEDULE_EVENT.fleet_id, SCHEDULE_EVENT.event_id);
  await act(async () => {pending.resolve({ok: true, data: {...SCHEDULE_EVENT, status: "received"}}); await pending.promise;});
  expect(await screen.findByLabelText("Event in progress")).toBeTruthy();
  read.mockResolvedValue({ok: true, data: {...SCHEDULE_EVENT, response_text: "Support queue reviewed."}});
  view.rerender(<TooltipProvider><FleetScopeProvider workspaceId={SCHEDULE_EVENT.workspace_id} fleetId={SCHEDULE_EVENT.fleet_id}>
    <FleetRunDetails eventId={SCHEDULE_EVENT.event_id} status="processed" />
  </FleetScopeProvider></TooltipProvider>);
  expect(await screen.findByText("Support queue reviewed.")).toBeTruthy();
  expect(screen.getByLabelText("Successful event")).toBeTruthy();
  expect(screen.getByRole("dialog", {name: "Event details"})).toBeTruthy();
  expect(screen.getByText("Schedule")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Close"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(read).toHaveBeenCalledTimes(2);
  view.unmount();
});

it("keeps a refused native read distinct from a completed run without a result", async () => {
  read.mockResolvedValue({ok: false, error: "not found"});
  render(<FleetScopeProvider workspaceId={SCHEDULE_EVENT.workspace_id} fleetId={SCHEDULE_EVENT.fleet_id}>
    <FleetRunDetails eventId={SCHEDULE_EVENT.event_id} status="fleet_error" />
  </FleetScopeProvider>);
  fireEvent.click(screen.getByRole("button", {name: "Run details"}));
  expect(await screen.findByText("Run details unavailable. Close and try again.")).toBeTruthy();
  expect(screen.queryByText("No result recorded")).toBeNull();
  expect(screen.queryByText("Completed.")).toBeNull();
  expect(read).toHaveBeenCalledOnce();
});
