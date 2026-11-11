import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FleetLifecycleDialog } from "@/components/layout/FleetLifecycleDialog";
import { FLEET_DETAIL, WORKSPACE, response } from "./helpers/draft-fixtures";
import { FleetNoticeProvider } from "@/components/layout/FleetNoticeProvider";
import { advanceFleetLifecycle } from "@/lib/fleet-lifecycle";
import { useState } from "react";

const {token} = vi.hoisted(() => ({token: vi.fn()}));
vi.mock("@/lib/auth/credential", () => ({credential: token}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => ({userId: "owner", isLoaded: true, isSignedIn: true})}));
const network = vi.fn<typeof fetch>();
const ARCHIVED = {...FLEET_DETAIL, status: "killed", etag: '"confirmed-stop"'};
const onRemoved = vi.fn();
beforeEach(() => {
  vi.resetAllMocks(); token.mockResolvedValue("unit-test-bearer"); vi.stubGlobal("fetch", network);
});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

async function confirm() {
  render(<FleetLifecycleDialog action="delete" workspace={WORKSPACE} fleet={FLEET_DETAIL} onRemoved={onRemoved} onClose={() => {}} />);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", {name: "Permanently delete"}));
  return user;
}

it("uses one confirmation for an active fleet and carries the stop response ETag to DELETE", async () => {
  network.mockResolvedValueOnce(response({...FLEET_DETAIL, status: "active"}))
    .mockResolvedValueOnce(response(ARCHIVED)).mockResolvedValueOnce(new Response(null, {status: 204}));
  await confirm();
  await waitFor(() => expect(onRemoved).toHaveBeenCalledExactlyOnceWith(FLEET_DETAIL.id));
  const methods = network.mock.calls.map(([, init]) => init?.method);
  expect(methods).toEqual(["GET", "PATCH", "DELETE"]);
  const [read, stop, remove] = network.mock.calls;
  expect(read?.[0]).toBe(`/live/v1/workspaces/${WORKSPACE}/fleets/${FLEET_DETAIL.id}`);
  expect(new Headers(stop?.[1]?.headers).get("If-Match")).toBe(FLEET_DETAIL.etag);
  expect(new Headers(remove?.[1]?.headers).get("If-Match")).toBe(ARCHIVED.etag);
  const target = remove?.[0];
  const path = typeof target === "string" ? target : target instanceof URL ? target.href : target?.url;
  expect(path).toContain(`/v1/workspaces/${WORKSPACE}/fleets/${FLEET_DETAIL.id}`);
});

it("retains history after deletion fails and never repeats the confirmed stop", async () => {
  network.mockResolvedValueOnce(response({...FLEET_DETAIL, status: "active"}))
    .mockResolvedValueOnce(response(ARCHIVED))
    .mockResolvedValueOnce(response({user_message: "Deletion unavailable"}, 503))
    .mockResolvedValueOnce(response(ARCHIVED))
    .mockResolvedValueOnce(new Response(null, {status: 204}));
  const user = await confirm();
  expect((await screen.findByRole("alert")).textContent).toContain("History is retained");
  expect(onRemoved).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", {name: "Check status"}));
  const dialog = within(screen.getByRole("alertdialog"));
  await user.click(await dialog.findByRole("button", {name: "Retry deletion"}));
  await waitFor(() => expect(onRemoved).toHaveBeenCalledOnce());
  expect(network.mock.calls.map(([, init]) => init?.method)).toEqual(["GET", "PATCH", "DELETE", "GET", "DELETE"]);
});

it("rejects a mutation when the server credential is absent and leaves confirmation open", async () => {
  token.mockResolvedValue(null);
  network.mockResolvedValueOnce(response({...FLEET_DETAIL, status: "active"}));
  await confirm();
  expect((await screen.findByRole("alert")).textContent).toMatch(/session expired/i);
  expect(network.mock.calls.map(([, init]) => init?.method)).toEqual(["GET"]);
  expect(onRemoved).not.toHaveBeenCalled();
});

it("archives without deleting history and confirms the history link", async () => {
  network.mockResolvedValueOnce(response({...FLEET_DETAIL, status: "active"})).mockResolvedValueOnce(response(ARCHIVED));
  function Archive() {
    const [visible, setVisible] = useState(true);
    return <FleetNoticeProvider>{visible ? <FleetLifecycleDialog action="archive" workspace={WORKSPACE} fleet={FLEET_DETAIL}
      onRemoved={id => {onRemoved(id); setVisible(false);}} onClose={() => setVisible(false)} /> : null}</FleetNoticeProvider>;
  }
  render(<Archive />);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", {name: "Archive"}));
  await waitFor(() => expect(onRemoved).toHaveBeenCalledOnce());
  expect(network.mock.calls.map(([, init]) => init?.method)).toEqual(["GET", "PATCH"]);
  expect(screen.getByText("Fleet archived")).toBeTruthy();
  expect(screen.getByRole("link", {name: "View history"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/${FLEET_DETAIL.id}`);
  await user.click(screen.getByRole("button", {name: "Dismiss notification"}));
});

it("checks a contradictory stop response before allowing any deletion", async () => {
  network.mockResolvedValueOnce(response({...FLEET_DETAIL, status: "active"}))
    .mockResolvedValueOnce(response({...FLEET_DETAIL, status: "stopped"}))
    .mockResolvedValueOnce(response({...FLEET_DETAIL, status: "active"}));
  const user = await confirm();
  expect((await screen.findByRole("alert")).textContent).toContain("Permanent stop could not be confirmed");
  await user.click(screen.getByRole("button", {name: "Check status"}));
  expect((await screen.findByRole("alert")).textContent).toContain("still running or stopped temporarily");
  expect(screen.getByRole("button", {name: "Try again"})).toBeTruthy();
  expect(network.mock.calls.map(([, init]) => init?.method)).toEqual(["GET", "PATCH", "GET"]);
  expect(onRemoved).not.toHaveBeenCalled();
});

it("retries a refused deletion from its retained ETag without another permanent stop", async () => {
  network.mockResolvedValueOnce(response(ARCHIVED))
    .mockResolvedValueOnce(response({user_message: "Delete refused"}, 422))
    .mockResolvedValueOnce(new Response(null, {status: 204}));
  const user = await confirm();
  await user.click(await screen.findByRole("button", {name: "Retry deletion"}));
  await waitFor(() => expect(onRemoved).toHaveBeenCalledOnce());
  expect(network.mock.calls.map(([, init]) => init?.method)).toEqual(["GET", "DELETE", "DELETE"]);
});

it("returns a changed fleet to review after an uncertain deletion", async () => {
  network.mockResolvedValueOnce(response(ARCHIVED))
    .mockResolvedValueOnce(response({user_message: "Conflict"}, 412))
    .mockResolvedValueOnce(response({...FLEET_DETAIL, status: "active"}));
  const user = await confirm();
  await user.click(await screen.findByRole("button", {name: "Check status"}));
  expect((await screen.findByRole("alert")).textContent).toContain("fleet has changed");
  expect(screen.getByRole("button", {name: "Try again"})).toBeTruthy();
  expect(onRemoved).not.toHaveBeenCalled();
});

it.each(["initial", "after-delete"])("confirms an absent fleet at %s without guessing another mutation", async when => {
  if (when === "after-delete") network.mockResolvedValueOnce(response(ARCHIVED))
    .mockResolvedValueOnce(response({user_message: "Response lost"}, 503));
  network.mockResolvedValueOnce(response({user_message: "Missing"}, 404));
  const user = await confirm();
  if (when === "after-delete") await user.click(await screen.findByRole("button", {name: "Check status"}));
  await waitFor(() => expect(onRemoved).toHaveBeenCalledOnce());
  expect(network.mock.calls.map(([, init]) => init?.method)).toEqual(when === "initial" ? ["GET"] : ["GET", "DELETE", "GET"]);
});

it("keeps a failed read retryable and sends no write", async () => {
  network.mockResolvedValueOnce(response({user_message: "Read unavailable"}, 503));
  await confirm();
  expect((await screen.findByRole("alert")).textContent).toContain("Read unavailable");
  expect(screen.getByRole("button", {name: "Try again"})).toBeTruthy();
  expect(network.mock.calls.map(([, init]) => init?.method)).toEqual(["GET"]);
  expect(onRemoved).not.toHaveBeenCalled();
});

it.each(["read", "stop", "delete"])("abandons a late %s response after leaving its confirmation", async stage => {
  const pending = Promise.withResolvers<Response>();
  if (stage === "stop") network.mockResolvedValueOnce(response({...FLEET_DETAIL, status: "active"}));
  if (stage === "delete") network.mockResolvedValueOnce(response(ARCHIVED));
  network.mockReturnValueOnce(pending.promise);
  await confirm();
  await waitFor(() => expect(network).toHaveBeenCalledTimes(stage === "read" ? 1 : 2));
  cleanup();
  await act(async () => {pending.resolve(stage === "delete" ? new Response(null, {status: 204}) : response(ARCHIVED));});
  expect(onRemoved).not.toHaveBeenCalled();
  expect(network).toHaveBeenCalledTimes(stage === "read" ? 1 : 2);
});

it("does no I/O when lifecycle cancellation predates submission", async () => {
  const controller = new AbortController(); controller.abort();
  expect(await advanceFleetLifecycle({workspace: WORKSPACE, fleetId: FLEET_DETAIL.id, action: "delete", signal: controller.signal}, {stage: "read"})).toEqual({kind: "cancelled"});
  expect(network).not.toHaveBeenCalled();
});

it("keeps the action unconfirmed if the browser cannot compose its cancellation signal", async () => {
  const any = vi.spyOn(AbortSignal, "any").mockImplementationOnce(() => {throw new TypeError("Signal composition unavailable");});
  try {
    await confirm();
    expect((await screen.findByRole("alert")).textContent).toContain("We couldn’t confirm this request");
    expect(network).not.toHaveBeenCalled(); expect(onRemoved).not.toHaveBeenCalled();
  } finally {any.mockRestore();}
});
