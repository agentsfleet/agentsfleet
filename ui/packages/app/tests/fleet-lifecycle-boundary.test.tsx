import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FleetLifecycleDialog } from "@/components/layout/FleetLifecycleDialog";
import { FLEET_DETAIL, WORKSPACE, response } from "./helpers/draft-fixtures";

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
