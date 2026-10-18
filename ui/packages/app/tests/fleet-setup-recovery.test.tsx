import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FleetRecovery } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetRecovery";
import { SetupPanel } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/SetupPanel";
import { FLEET, FLEET_DETAIL, WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";

const {resume, refresh} = vi.hoisted(() => ({resume: vi.fn(), refresh: vi.fn()}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({setFleetStatusAction: resume}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh})}));
afterEach(() => {cleanup(); vi.resetAllMocks(); vi.unstubAllGlobals();});

it.each([200, 403, 412])("reconciles setup continuation status %i without hiding the fleet", async (status) => {
  resume.mockResolvedValue(status === 200 ? {ok: true} : {ok: false, error: "Denied", status});
  render(<FleetRecovery fleet={FLEET} workspace={WORKSPACE} />);
  fireEvent.click(screen.getByRole("button", {name: "Continue setup"}));
  await waitFor(() => expect(screen.getByRole("button", {name: "Continue setup"})).toHaveProperty("disabled", false));
  expect(refresh).toHaveBeenCalledOnce();
  expect(screen.getByRole("link", {name: "Open fleet"})).toBeTruthy();
  expect(resume).toHaveBeenCalledWith(WORKSPACE, FLEET.id, "active", FLEET.etag);
  if (status === 403) expect(screen.getByRole("alert").textContent).toBe("Denied");
  else expect(screen.queryByRole("alert")).toBeNull();
});

it.each([new Error("Transport failed"), "uncertain"])("retains a failed continuation and permits retry", async (cause) => {
  resume.mockRejectedValueOnce(cause).mockResolvedValueOnce({ok: true});
  render(<FleetRecovery fleet={FLEET} workspace={WORKSPACE} />);
  fireEvent.click(screen.getByRole("button", {name: "Continue setup"}));
  await screen.findByRole("alert");
  fireEvent.click(screen.getByRole("button", {name: "Continue setup"}));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(screen.getByRole("link", {name: "Open fleet"})).toBeTruthy();
});

it("shows recovery only for unfinished setup or first-run admission", () => {
  const view = render(<FleetRecovery fleet={{...FLEET, status: "stopped"}} workspace={WORKSPACE} />);
  expect(screen.queryByRole("link")).toBeNull();
  view.rerender(<FleetRecovery fleet={{...FLEET, status: "active", start: {kind: "run_now", state: "pending"}}} workspace={WORKSPACE} />);
  expect(screen.getByRole("button", {name: "Continue setup"})).toBeTruthy();
  view.rerender(<FleetRecovery fleet={{...FLEET, status: "active", start: {kind: "run_now", state: "accepted"}}} workspace={WORKSPACE} />);
  expect(screen.queryByRole("link")).toBeNull();
});

it("pins improvement to the displayed revision and requires renewed access review", () => {
  render(<SetupPanel fleet={FleetDetailSchema.parse({...FLEET_DETAIL, needs_re_review: true})} workspaceId={WORKSPACE} />);
  expect(screen.getByText("Review current access before resuming")).toBeTruthy();
  const href = screen.getByRole("link", {name: "Edit fleet"}).getAttribute("href");
  expect(href).toContain(`fleet=${FLEET.id}&setup_revision=${FLEET.setup_revision}`);
});

it("keeps saved configuration available through a keyboard-operated disclosure", async () => {
  const user = userEvent.setup();
  const fleet = FleetDetailSchema.parse(FLEET_DETAIL);
  render(<SetupPanel fleet={fleet} workspaceId={WORKSPACE} />);
  const details = screen.getByRole("button", {name: "Advanced settings"});
  expect(details.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByLabelText("Saved configuration")).toBeNull();
  details.focus();
  await user.keyboard("{Enter}");
  expect(details.getAttribute("aria-expanded")).toBe("true");
  expect(screen.getAllByLabelText("Saved configuration").at(-1)?.textContent).toContain(JSON.stringify(fleet.definition.config, null, 2));
  await user.keyboard(" ");
  expect(details.getAttribute("aria-expanded")).toBe("false");
  expect(document.activeElement).toBe(details);
});

it.each(["installing", "active", "stopped"] as const)("retries an unregistered schedule while %s and reports a refusal", async (status) => {
  const failed = {schedule_id: "sched-1", fleet_id: FLEET.id, sync: "failed", last_error: "refused"};
  const fetchMock = vi.fn(async (_input: string, _init?: RequestInit) => new Response(JSON.stringify({...failed, sync: "synced", last_error: null}), {status: 200}));
  vi.stubGlobal("fetch", fetchMock);
  render(<FleetRecovery fleet={{...FLEET, status, start: {kind: "run_now", state: "accepted"}, schedules: [failed]}} workspace={WORKSPACE} />);
  fireEvent.click(screen.getByRole("button", {name: "Retry registration"}));
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce());
  expect(fetchMock.mock.calls[0]?.[0]).toBe(`/live/v1/workspaces/${WORKSPACE}/fleets/${FLEET.id}/schedules/sched-1/sync`);
  fetchMock.mockResolvedValueOnce(new Response(JSON.stringify({error_code: "UZ-X", user_message: "Scheduler unavailable"}), {status: 503}));
  fireEvent.click(screen.getByRole("button", {name: "Retry registration"}));
  expect((await screen.findByText("Scheduler unavailable")).textContent).toBe("Scheduler unavailable");
  vi.unstubAllGlobals();
});

// Value: protects=active fleets expose retry for every unconfirmed schedule state;
// fails_when=syncing or unknown states lose recovery, or synced schedules offer retry;
// why_new=the fleet-page retry scenario covers failed registrations only; seam=none;
// no_claim=external scheduler registration (HTTP response is fixture controlled).
it.each(["failed", "syncing", "synced", "unconfirmed"])("reconciles an active fleet's %s schedule registration", async (sync) => {
  const schedule = {schedule_id: "sched-matrix", fleet_id: FLEET.id, sync, last_error: null};
  const fetchMock = vi.fn(async () => new Response(JSON.stringify({...schedule, sync: "synced"}), {status: 200}));
  vi.stubGlobal("fetch", fetchMock);
  render(<FleetRecovery fleet={{...FLEET, status: "active", start: {kind: "run_now", state: "accepted"}, schedules: [schedule]}} workspace={WORKSPACE} />);
  const retry = screen.queryByRole("button", {name: "Retry registration"});
  if (sync === "synced") {
    expect(retry).toBeNull();
    expect(screen.queryByLabelText("Schedule registration")).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
    return;
  }
  expect(retry).toBeTruthy();
  const warning = sync === "failed" ? "is saved but not registered, so it will not fire"
    : "is saved but its registration was not confirmed, so it may not fire";
  expect(screen.getByText(new RegExp(warning))).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Retry registration"}));
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce());
  expect(fetchMock).toHaveBeenCalledOnce();
  expect(fetchMock).toHaveBeenCalledWith(`/live/v1/workspaces/${WORKSPACE}/fleets/${FLEET.id}/schedules/sched-matrix/sync`, expect.objectContaining({method: "POST"}));
  expect(resume).not.toHaveBeenCalled();
  expect(screen.getAllByRole("alert")).toHaveLength(1);
  expect(screen.getByRole("alert", {name: "Schedule registration"})).toBeTruthy();
});
