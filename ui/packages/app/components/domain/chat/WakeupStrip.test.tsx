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

it.each(["trigger", "api"])("changes only edited fields and preserves existing %s overrides", async source => {
  const saved = {...current, source, timezone: "Europe/London", override: {timezone: "Europe/London", status: "active"}};
  const fetcher = vi.fn().mockImplementation(async () => Response.json(saved));
  vi.stubGlobal("fetch", fetcher); mount();
  for (const [label, value, field] of [
    ["What should the fleet do?", "Check the deployment", "message"],
    ["Schedule", "17 5 * * 1", "cron"],
    ["Timezone", "America/New_York", "timezone"],
  ] as const) {
    fireEvent.click(screen.getByRole("button", {name: "Edit wake-up"}));
    fireEvent.change(await screen.findByLabelText(label), {target: {value}});
    fireEvent.click(screen.getByRole("button", {name: "Save"}));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const request = fetcher.mock.calls.at(-1)![1];
    const changed = {[field]: value};
    expect(JSON.parse(request.body)).toEqual(source === "trigger" ? {override: {...saved.override, ...changed}} : changed);
    expect(request.headers["If-Match"]).toBe(saved.etag);
  }
});

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
  const pending = Promise.withResolvers<Response>();
  const fetcher = vi.fn().mockReturnValue(pending.promise);
  vi.stubGlobal("fetch", fetcher); mount();
  fireEvent.click(screen.getByRole("button", {name: "Delete wake-up"}));
  expect(fetcher).not.toHaveBeenCalled();
  fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", {name: "Delete wake-up"}));
  await waitFor(() => expect(within(screen.getByRole("alertdialog")).getByRole("button", {name: "Cancel"})).toHaveProperty("disabled", true));
  fireEvent.keyDown(screen.getByRole("alertdialog"), {key: "Escape"});
  expect(screen.getByRole("alertdialog")).toBeTruthy();
  await act(async () => pending.resolve(Response.json({...current, status: "deleting", sync: "syncing"})));
  await screen.findByText("Removal pending.");
  expect(screen.getByRole("button", {name: "Edit wake-up"})).toHaveProperty("disabled", true);
  expect(screen.queryByRole("alertdialog")).toBeNull();
});

it("dismisses an unsubmitted wake-up editor with Escape without writing", async () => {
  const fetcher = vi.fn().mockResolvedValue(Response.json(current));
  vi.stubGlobal("fetch", fetcher); mount();
  fireEvent.click(screen.getByRole("button", {name: "Edit wake-up"}));
  await screen.findByLabelText("Schedule");
  fireEvent.keyDown(screen.getByRole("dialog"), {key: "Escape"});
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fetcher).toHaveBeenCalledOnce();
});

it("keeps a pending edit open and excludes a second programmatic submission", async () => {
  const pending = Promise.withResolvers<Response>();
  const fetcher = vi.fn().mockResolvedValueOnce(Response.json(current)).mockReturnValueOnce(pending.promise);
  vi.stubGlobal("fetch", fetcher); mount();
  fireEvent.click(screen.getByRole("button", {name: "Edit wake-up"}));
  const input = await screen.findByLabelText("Schedule");
  fireEvent.click(screen.getByRole("button", {name: "Save"}));
  await waitFor(() => expect(input).toHaveProperty("disabled", true));
  fireEvent.keyDown(screen.getByRole("dialog"), {key: "Escape"});
  expect(screen.getByRole("dialog")).toBeTruthy();
  fireEvent.submit(input.closest("form")!);
  expect(fetcher).toHaveBeenCalledTimes(2);
  await act(async () => pending.resolve(Response.json(current)));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mocks.refresh).toHaveBeenCalledOnce();
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

it("pauses and resumes against the latest saved version, then reconciles a server update", async () => {
  const paused = {...current, status: "paused", run_state: "paused", next_run_at: null};
  const fetcher = vi.fn().mockResolvedValueOnce(Response.json(current)).mockResolvedValueOnce(Response.json(paused))
    .mockResolvedValueOnce(Response.json({...paused, etag: '"two"'})).mockResolvedValueOnce(Response.json(current));
  vi.stubGlobal("fetch", fetcher); const view = mount();
  fireEvent.click(screen.getByRole("button", {name: "Pause wake-up"}));
  fireEvent.click(await screen.findByRole("button", {name: "Resume wake-up"}));
  await screen.findByRole("button", {name: "Pause wake-up"});
  expect(JSON.parse(fetcher.mock.calls[1]?.[1].body)).toEqual({override: {status: "paused"}});
  expect(JSON.parse(fetcher.mock.calls[3]?.[1].body)).toEqual({override: {status: "active"}});
  expect(fetcher.mock.calls[3]?.[1].headers["If-Match"]).toBe('"two"');
  mocks.view.schedules = [paused]; view.rerender(<WakeupStrip />);
  expect(screen.getByRole("button", {name: "Resume wake-up"})).toBeTruthy();
});

it("retries a refused read, discards an abandoned edit and confirms a completed removal", async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(Response.json({user_message: "Read unavailable"}, {status: 503}))
    .mockResolvedValueOnce(Response.json(current)).mockResolvedValueOnce(new Response(null, {status: 204}));
  vi.stubGlobal("fetch", fetcher); mount();
  fireEvent.click(screen.getByRole("button", {name: "Edit wake-up"}));
  await screen.findByText("Read unavailable");
  fireEvent.click(screen.getByRole("button", {name: "Refresh schedule"}));
  expect(screen.queryByRole("alert")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Edit wake-up"}));
  await screen.findByLabelText("Schedule");
  fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
  expect(screen.queryByRole("dialog")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Delete wake-up"}));
  fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
  expect(fetcher).toHaveBeenCalledTimes(2);
  fireEvent.click(screen.getByRole("button", {name: "Delete wake-up"}));
  fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", {name: "Delete wake-up"}));
  await waitFor(() => expect(screen.queryByRole("group")).toBeNull());
});

it("prioritizes unconfirmed schedules, hides expired entries and labels unnamed wake-ups", () => {
  mocks.view.schedules = [current, {...current, schedule_id: "expired", status: "expired"},
    {...current, schedule_id: "removed", status: "removed"},
    {...current, schedule_id: "pending", description: "", sync: "syncing", next_run_at: null},
    {...current, schedule_id: "paused", status: "paused", next_run_at: null}];
  mount();
  expect(screen.getByRole("group", {name: "Scheduled wake-up"})).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "2 more wake-ups"}));
  expect(screen.getAllByRole("group")).toHaveLength(3);
});

it("ignores a read rejection after the fleet has unmounted", async () => {
  const pending = Promise.withResolvers<Response>(); vi.stubGlobal("fetch", vi.fn(() => pending.promise));
  const view = mount(); fireEvent.click(screen.getByRole("button", {name: "Edit wake-up"}));
  view.unmount(); await act(async () => pending.reject(new Error("Connection closed")));
  expect(screen.queryByRole("alert")).toBeNull(); expect(mocks.refresh).not.toHaveBeenCalled();
});
