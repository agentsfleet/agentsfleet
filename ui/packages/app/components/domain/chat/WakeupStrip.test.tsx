import {act, cleanup, fireEvent, render, screen, waitFor, within} from "@testing-library/react";
import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {TooltipProvider} from "@agentsfleet/design-system";
import {SCHEDULE, WORKSPACE, FLEET_ID} from "@/tests/helpers/draft-fixtures";
import type {Schedule} from "@/lib/api/draft-types";
import {WakeupStrip} from "./WakeupStrip";

const mocks = vi.hoisted(() => ({refresh: vi.fn(), view: {subject: "user", workspaceId: "", fleetId: "", schedules: [] as Schedule[]}}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh: mocks.refresh})}));
vi.mock("./FleetTaskProvider", () => ({useFleetTaskView: () => mocks.view}));
const current = {...SCHEDULE, source: "trigger", etag: '"one"', description: "Check release", next_run_at: "2099-11-12T13:30:00Z"};
beforeEach(() => {mocks.refresh.mockClear(); mocks.view = {subject: "user", workspaceId: WORKSPACE, fleetId: FLEET_ID, schedules: [current]};});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});
function mount() {return render(<WakeupStrip />, {wrapper: TooltipProvider});}

// Value: protects=editable concurrency-safe wake-up controls; fails_when=a stale form overwrites another edit or a late read opens in another account;
// why_new=the approved composer exposes these actions; seam=HTTP; no_claim=live cron delivery.
it("keeps a stale form visible and reloads its saved version before retrying", async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(Response.json(current))
    .mockResolvedValueOnce(Response.json({user_message: "Schedule changed elsewhere"}, {status: 412}))
    .mockResolvedValueOnce(Response.json({...current, etag: '"two"', cron: "0 8 * * *"}))
    .mockResolvedValueOnce(Response.json({...current, cron: "0 8 * * *"}));
  vi.stubGlobal("fetch", fetcher); mount();
  fireEvent.click(screen.getByRole("button", {name: "Edit wake-up"}));
  const schedule = await screen.findByLabelText("Schedule");
  fireEvent.change(schedule, {target: {value: "0 9 * * *"}});
  fireEvent.click(screen.getByRole("button", {name: "Save"}));
  await screen.findByText("Schedule changed elsewhere");
  expect(schedule).toHaveProperty("value", "0 9 * * *");
  fireEvent.click(screen.getByRole("button", {name: "Reload saved schedule"}));
  await waitFor(() => expect(screen.getByLabelText("Schedule")).toHaveProperty("value", "0 8 * * *"));
  fireEvent.click(screen.getByRole("button", {name: "Save"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(fetcher.mock.calls[3]![1].headers["If-Match"]).toBe('"two"');
  expect(mocks.refresh).toHaveBeenCalledOnce();
});

it("requires deletion confirmation and retains pending deregistration visibly", async () => {
  const fetcher = vi.fn().mockResolvedValue(Response.json({...current, status: "deleting", sync: "syncing"}));
  vi.stubGlobal("fetch", fetcher); mount();
  fireEvent.click(screen.getByRole("button", {name: "Delete wake-up"}));
  expect(fetcher).not.toHaveBeenCalled();
  fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", {name: "Delete wake-up"}));
  await screen.findByText("Removal pending.");
  expect(screen.getByRole("button", {name: "Edit wake-up"})).toHaveProperty("disabled", true);
  expect(screen.queryByRole("alertdialog")).toBeNull();
});

it("discards a late editor read when its owning fleet unmounts", async () => {
  const pending = Promise.withResolvers<Response>(); const fetcher = vi.fn().mockReturnValue(pending.promise);
  vi.stubGlobal("fetch", fetcher); const view = mount();
  fireEvent.click(screen.getByRole("button", {name: "Edit wake-up"}));
  view.unmount();
  expect(fetcher.mock.calls[0]![1].signal.aborted).toBe(true);
  await act(async () => pending.resolve(Response.json(current)));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(mocks.refresh).not.toHaveBeenCalled();
});

it("retains unsaved input and its original ETag across a schedule refresh", async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(Response.json(current))
    .mockResolvedValueOnce(Response.json({user_message: "Schedule changed elsewhere"}, {status: 412}));
  vi.stubGlobal("fetch", fetcher); const view = mount();
  fireEvent.click(screen.getByRole("button", {name: "Edit wake-up"}));
  fireEvent.change(await screen.findByLabelText("Schedule"), {target: {value: "0 9 * * *"}});
  mocks.view.schedules = [{...current, status: "paused", next_run_at: null}, {...current, schedule_id: "next", next_run_at: "2099-11-12T14:00:00Z"}];
  view.rerender(<WakeupStrip />);
  expect(screen.getByLabelText("Schedule")).toHaveProperty("value", "0 9 * * *");
  fireEvent.click(screen.getByRole("button", {name: "Save"}));
  await screen.findByText("Schedule changed elsewhere");
  expect(fetcher.mock.calls[1]?.[1].headers["If-Match"]).toBe('"one"');
});

it("keeps the next schedule visible and reveals the rest through the native disclosure", () => {
  mocks.view.schedules = [current, {...current, schedule_id: "later", next_run_at: "2099-11-12T14:00:00Z"}];
  mount();
  expect(screen.getAllByRole("button", {name: "Edit wake-up"})).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", {name: "1 more wake-ups"}));
  expect(screen.getAllByRole("button", {name: "Edit wake-up"})).toHaveLength(2);
});
