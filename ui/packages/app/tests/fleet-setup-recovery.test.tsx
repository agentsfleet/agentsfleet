import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { FleetRecovery } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetRecovery";
import { SetupPanel } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/SetupPanel";
import { FLEET, FLEET_DETAIL, WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";

const {resume, refresh} = vi.hoisted(() => ({resume: vi.fn(), refresh: vi.fn()}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({setFleetStatusAction: resume}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh})}));
afterEach(() => {cleanup(); vi.resetAllMocks();});

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
  const href = screen.getByRole("link", {name: "Improve in private chat"}).getAttribute("href");
  expect(href).toContain(`fleet=${FLEET.id}&setup_revision=${FLEET.setup_revision}`);
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
