import React from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FleetActions } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetActions";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { FLEET_DETAIL, response } from "./helpers/draft-fixtures";

const {setStatus, remove, navigation} = vi.hoisted(() => ({setStatus: vi.fn(), remove: vi.fn(), navigation: {replace: vi.fn(), refresh: vi.fn()}}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({setFleetStatusAction: setStatus, deleteFleetAction: remove}));
vi.mock("next/navigation", () => ({useRouter: () => navigation}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => ({userId: "owner", isLoaded: true, isSignedIn: true})}));
const FLEET = FleetDetailSchema.parse({...FLEET_DETAIL, status: "active"});
const network = vi.fn<typeof fetch>();

beforeEach(() => {
  vi.resetAllMocks(); vi.stubGlobal("fetch", network);
  network.mockResolvedValue(response(FLEET_DETAIL));
  setStatus.mockImplementation(async (_workspace, _id, status) => ({ok: true, data: {...FLEET, status}}));
  remove.mockResolvedValue({ok: true, data: undefined});
});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

async function menu(status = "active") {
  render(React.createElement(FleetActions, {workspaceId: "release", fleet: {...FLEET, status}}));
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", {name: "Fleet actions"}));
  return user;
}
async function choose(user: ReturnType<typeof userEvent.setup>, label: string) {
  await user.click(screen.getByRole("menuitem", {name: label}));
  return within(screen.getByRole("alertdialog"));
}

it("offers independent Archive and Permanently delete actions on an active fleet", async () => {
  await menu();
  expect(screen.getByRole("menuitem", {name: "Archive"})).toBeTruthy();
  expect(screen.getByRole("menuitem", {name: "Permanently delete"})).toBeTruthy();
  expect(screen.getByRole("menuitem", {name: "Stop"})).toBeTruthy();
  expect(screen.queryByRole("menuitem", {name: "Kill"})).toBeNull();
  expect(setStatus).not.toHaveBeenCalled(); expect(remove).not.toHaveBeenCalled();
});

it("keeps permanent deletion available in archived history without Restore", async () => {
  await menu("killed");
  expect(screen.getByRole("menuitem", {name: "Permanently delete"})).toBeTruthy();
  for (const name of ["Archive", "Restore", "Resume", "Stop"]) expect(screen.queryByRole("menuitem", {name})).toBeNull();
});

it("keeps an unfamiliar lifecycle state free of guessed destructive actions", async () => {
  await menu("draining");
  expect(screen.queryByRole("menuitem")).toBeNull();
  expect(setStatus).not.toHaveBeenCalled(); expect(remove).not.toHaveBeenCalled();
});

it.each(["stopped", "paused"])("preserves Resume for a temporarily %s fleet", async status => {
  const user = await menu(status);
  const dialog = await choose(user, "Resume");
  await user.click(dialog.getByRole("button", {name: "Resume"}));
  await waitFor(() => expect(navigation.refresh).toHaveBeenCalledOnce());
  expect(setStatus).toHaveBeenCalledExactlyOnceWith("release", FLEET.id, "active", FLEET.etag);
});

it("requires confirmation before a temporary stop and sends the observed ETag", async () => {
  const user = await menu();
  const dialog = await choose(user, "Stop");
  expect(setStatus).not.toHaveBeenCalled();
  await user.click(dialog.getByRole("button", {name: "Stop"}));
  await waitFor(() => expect(navigation.refresh).toHaveBeenCalledOnce());
  expect(setStatus).toHaveBeenCalledExactlyOnceWith("release", FLEET.id, "stopped", FLEET.etag);
});

it("shows a refusal without navigating away or claiming a status change", async () => {
  setStatus.mockResolvedValue({ok: false, status: 403, error: "Access denied"});
  const user = await menu();
  const dialog = await choose(user, "Stop");
  await user.click(dialog.getByRole("button", {name: "Stop"}));
  expect((await screen.findByRole("alert")).textContent).toContain("Access denied");
  expect(navigation.replace).not.toHaveBeenCalled(); expect(navigation.refresh).not.toHaveBeenCalled();
  expect(screen.getByRole("alertdialog")).toBeTruthy();
});

it("discards a late response after unmount and excludes duplicate stop submissions", async () => {
  const pending = Promise.withResolvers<unknown>(); setStatus.mockReturnValueOnce(pending.promise);
  const user = await menu(); const dialog = await choose(user, "Stop");
  const button = dialog.getByRole("button", {name: "Stop"});
  act(() => {fireEvent.click(button); fireEvent.click(button);});
  expect(setStatus).toHaveBeenCalledOnce();
  cleanup(); pending.resolve({ok: true, data: {...FLEET, status: "stopped"}});
  await pending.promise;
  expect(navigation.refresh).not.toHaveBeenCalled();
});

it("keeps cancellation free of lifecycle writes", async () => {
  const user = await menu(); const dialog = await choose(user, "Permanently delete");
  expect(dialog.getByText(/chat, activity and fleet memory/)).toBeTruthy();
  await user.click(dialog.getByRole("button", {name: "Cancel"}));
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(network).not.toHaveBeenCalled(); expect(remove).not.toHaveBeenCalled();
});

it.each([409, 412])("refreshes stale fleet state after a %s refusal while retaining the error", async status => {
  setStatus.mockResolvedValueOnce({ok: false, status, error: "Fleet changed"});
  const user = await menu();
  const dialog = await choose(user, "Stop");
  await user.click(dialog.getByRole("button", {name: "Stop"}));
  expect((await screen.findByRole("alert")).textContent).toContain("Fleet changed");
  expect(navigation.refresh).toHaveBeenCalledOnce();
  expect(navigation.replace).not.toHaveBeenCalled();
});

it.each(["contradictory", "thrown"])("keeps a %s stop response unconfirmed", async mode => {
  if (mode === "thrown") setStatus.mockRejectedValueOnce(new Error("Lost response"));
  else setStatus.mockResolvedValueOnce({ok: true, data: FLEET});
  const user = await menu();
  const dialog = await choose(user, "Stop");
  await user.click(dialog.getByRole("button", {name: "Stop"}));
  expect((await screen.findByRole("alert")).textContent).toContain("We couldn’t confirm this request.");
  expect(navigation.refresh).not.toHaveBeenCalled();
  expect(screen.getByRole("alertdialog")).toBeTruthy();
});

it("ignores a transport rejection after its action owner has unmounted", async () => {
  const pending = Promise.withResolvers<never>();
  setStatus.mockReturnValueOnce(pending.promise);
  const user = await menu();
  const dialog = await choose(user, "Stop");
  await user.click(dialog.getByRole("button", {name: "Stop"}));
  cleanup();
  await act(async () => {pending.reject(new Error("Disconnected"));});
  expect(navigation.refresh).not.toHaveBeenCalled();
  expect(navigation.replace).not.toHaveBeenCalled();
});

it.each(["Archive", "Stop"])("cancels %s without sending a lifecycle write", async label => {
  const user = await menu();
  const dialog = await choose(user, label);
  if (label === "Archive") expect(dialog.getByText(/cannot be restored/)).toBeTruthy();
  await user.click(dialog.getByRole("button", {name: "Cancel"}));
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(setStatus).not.toHaveBeenCalled();
  expect(remove).not.toHaveBeenCalled();
});

it("moves an archived open fleet to New fleet after its permanent stop is confirmed", async () => {
  const user = await menu(); const dialog = await choose(user, "Archive");
  await user.click(dialog.getByRole("button", {name: "Archive"}));
  await waitFor(() => expect(navigation.replace).toHaveBeenCalledExactlyOnceWith("/w/release/fleets/new"));
  expect(setStatus).toHaveBeenCalledExactlyOnceWith("release", FLEET.id, "killed", FLEET.etag);
  expect(remove).not.toHaveBeenCalled();
});
