import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { TooltipProvider } from "@agentsfleet/design-system";
import { FleetSettingsProvider, useFleetSettingsOwner } from "@/components/domain/fleet/FleetSettings";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { FLEET_DETAIL, WORKSPACE, response } from "@/tests/helpers/fleet-fixtures";
import { FleetHeader } from "./FleetHeader";

const navigation = vi.hoisted(() => ({refresh: vi.fn()}));
vi.mock("next/navigation", () => ({useRouter: () => navigation}));
const network = vi.fn<typeof fetch>();
const FLEET = FleetDetailSchema.parse(FLEET_DETAIL);
const RESOURCE = `/live/v1/workspaces/${WORKSPACE}/fleets/${FLEET.id}`;
const RENAMED = "release-review";
const actions = () => screen.getByRole("button", {name: "Fleet actions"});
const field = () => screen.getByRole("textbox", {name: "Fleet name"});

beforeEach(() => {vi.resetAllMocks(); vi.stubGlobal("fetch", network);});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

function Header({canGrant, status = FLEET.status}: {canGrant?: boolean; status?: string}) {
  const settings = useFleetSettingsOwner(WORKSPACE, FLEET.id, FLEET);
  return <TooltipProvider><FleetSettingsProvider value={settings}>
    <FleetHeader workspaceId={WORKSPACE} fleet={{...FLEET, status}} canGrant={canGrant} />
    <button type="button">Outside the header</button>
    <button type="button" onClick={() => {void settings.change("model", {model_ref: "different-model"});}}>Change model</button>
  </FleetSettingsProvider></TooltipProvider>;
}

it.each([[undefined, "active"], [true, "archived"]] as const)("keeps the name read-only without write access or when archived (%s, %s)", async (canGrant, status) => {
  const user = userEvent.setup();
  render(<Header canGrant={canGrant} status={status} />);
  expect(screen.getByRole("heading", {name: FLEET.name})).toBeTruthy();
  expect(screen.queryByRole("button", {name: FLEET.name})).toBeNull();
  await user.click(actions());
  expect(screen.queryByRole("menuitem", {name: "Rename fleet"})).toBeNull();
  expect(screen.queryByRole("menuitemcheckbox")).toBeNull();
});

it("opens the inline editor from Fleet actions and saves once when the field blurs", async () => {
  const pending = Promise.withResolvers<Response>();
  network.mockReturnValueOnce(pending.promise);
  const user = userEvent.setup();
  render(<Header canGrant />);
  await user.click(actions());
  expect(screen.getByRole("menuitemcheckbox", {name: "Read shared memories"})).toBeTruthy();
  await user.click(screen.getByRole("menuitem", {name: "Rename fleet"}));
  await waitFor(() => expect(document.activeElement).toBe(field()));
  await user.clear(field());
  await user.type(field(), "Release Review");
  await user.click(screen.getByRole("button", {name: "Outside the header"}));
  expect(field()).toHaveProperty("readOnly", true);
  expect(network).toHaveBeenCalledExactlyOnceWith(RESOURCE, expect.objectContaining({method: "PATCH",
    body: JSON.stringify({name: RENAMED}), headers: expect.objectContaining({"If-Match": FLEET.etag})}));
  pending.resolve(response({...FLEET, name: RENAMED, etag: '"renamed"'}));
  expect(await screen.findByRole("heading", {name: RENAMED})).toBeTruthy();
  expect(navigation.refresh).not.toHaveBeenCalled();
});

it("keeps the invalid draft until Escape and restores focus to the original name", async () => {
  const user = userEvent.setup();
  render(<Header canGrant />);
  await user.click(screen.getByRole("button", {name: FLEET.name}));
  await user.clear(field());
  await user.type(field(), "Review/Deploy{Enter}");
  expect(screen.getByRole("alert").textContent).toContain("Use up to 64 letters");
  await user.click(actions());
  await user.click(screen.getByRole("menuitem", {name: "Rename fleet"}));
  await waitFor(() => expect(document.activeElement).toBe(field()));
  expect(field()).toHaveProperty("value", "Review/Deploy");
  await user.keyboard("{Escape}");
  const title = screen.getByRole("button", {name: FLEET.name});
  await waitFor(() => expect(document.activeElement).toBe(title));
  expect(network).not.toHaveBeenCalled();
});

it("keeps the attempted name after a stale version and saves with the refreshed tag only on retry", async () => {
  const tag = '"edited-elsewhere"';
  network.mockResolvedValueOnce(response({user_message: "This fleet changed. Review the latest version."}, 412))
    .mockResolvedValueOnce(response({...FLEET, name: "outside-edit", etag: tag}))
    .mockResolvedValueOnce(response({...FLEET, name: RENAMED, etag: '"renamed"'}));
  const user = userEvent.setup();
  render(<Header canGrant />);
  await user.click(screen.getByRole("button", {name: FLEET.name}));
  await user.clear(field());
  await user.type(field(), "Release Review{Enter}");
  expect((await screen.findByRole("alert")).textContent).toBe("This fleet changed. Review the latest version.");
  expect(field()).toHaveProperty("value", "Release Review");
  expect(network).toHaveBeenCalledTimes(2);
  expect(network).toHaveBeenNthCalledWith(2, RESOURCE, expect.objectContaining({method: "GET"}));
  await user.keyboard("{Enter}");
  expect(await screen.findByRole("heading", {name: RENAMED})).toBeTruthy();
  expect(network).toHaveBeenLastCalledWith(RESOURCE, expect.objectContaining({method: "PATCH", headers: expect.objectContaining({"If-Match": tag})}));
});

it("opens with F2 and skips a save when the name is unchanged or cleared", async () => {
  const user = userEvent.setup();
  render(<Header canGrant />);
  const title = () => screen.getByRole("button", {name: FLEET.name});
  title().focus();
  await user.keyboard("{ArrowRight}");
  expect(screen.queryByRole("textbox", {name: "Fleet name"})).toBeNull();
  await user.keyboard("{F2}{Enter}");
  expect(document.activeElement).toBe(title());
  await user.keyboard("{F2}");
  await user.clear(field());
  await user.keyboard("{Enter}");
  expect(document.activeElement).toBe(title());
  expect(network).not.toHaveBeenCalled();
});

it("ignores composing keys and coalesces Enter and blur during a save", async () => {
  const pending = Promise.withResolvers<Response>();
  network.mockReturnValueOnce(pending.promise);
  const user = userEvent.setup();
  render(<Header canGrant />);
  await user.click(screen.getByRole("button", {name: FLEET.name}));
  await user.clear(field());
  await user.type(field(), RENAMED);
  fireEvent.keyDown(field(), {key: "Enter", isComposing: true});
  expect(network).not.toHaveBeenCalled();
  await user.keyboard("{Enter}{Escape}");
  expect(field()).toHaveProperty("readOnly", true);
  const outside = screen.getByRole("button", {name: "Outside the header"});
  await user.click(outside);
  expect(network).toHaveBeenCalledTimes(1);
  await act(async () => pending.resolve(response({...FLEET, name: RENAMED, etag: '"renamed"'})));
  expect(document.activeElement).toBe(outside);
  expect(screen.getByText("Fleet renamed.")).toBeTruthy();
  await waitFor(() => expect(screen.queryByText("Fleet renamed.")).toBeNull(), {timeout: 2000});
});

it("keeps the name draft during a model save and retries with its confirmed version", async () => {
  const pending = Promise.withResolvers<Response>();
  network.mockReturnValueOnce(pending.promise).mockResolvedValueOnce(response({...FLEET, name: RENAMED, etag: '"renamed"'}));
  const user = userEvent.setup();
  render(<Header canGrant />);
  await user.click(screen.getByRole("button", {name: "Change model"}));
  await user.click(screen.getByRole("button", {name: FLEET.name}));
  await user.clear(field());
  await user.type(field(), `${RENAMED}{Enter}`);
  expect((await screen.findByRole("alert")).textContent).toBe("Wait for the other fleet change to finish and try again.");
  expect(field()).toHaveProperty("value", RENAMED);
  expect(network).toHaveBeenCalledTimes(1);
  await act(async () => pending.resolve(response({...FLEET, model_ref: "different-model", etag: '"model-changed"'})));
  await user.keyboard("{Enter}");
  expect(await screen.findByRole("heading", {name: RENAMED})).toBeTruthy();
  expect(network).toHaveBeenLastCalledWith(RESOURCE, expect.objectContaining({headers: expect.objectContaining({"If-Match": '"model-changed"'})}));
});

it("confirms the saved name by reading it once after a lost response", async () => {
  network.mockRejectedValueOnce(new TypeError("connection lost"))
    .mockResolvedValueOnce(response({...FLEET, name: RENAMED, etag: '"renamed"'}));
  const user = userEvent.setup();
  render(<Header canGrant />);
  await user.click(screen.getByRole("button", {name: FLEET.name}));
  await user.clear(field());
  await user.type(field(), `${RENAMED}{Enter}`);
  const title = await screen.findByRole("button", {name: RENAMED});
  expect(document.activeElement).toBe(title);
  expect(network).toHaveBeenCalledTimes(2);
  expect(network).toHaveBeenLastCalledWith(RESOURCE, expect.objectContaining({method: "GET"}));
});

it.each([
  [200, {...FLEET, etag: '"edited"'}, "The fleet name changed. Review it and try again."],
  [409, {user_message: "That name is already in use."}, "That name is already in use."],
] as const)("keeps the draft when the response does not confirm it (%s)", async (status, body, message) => {
  network.mockResolvedValueOnce(response(body, status));
  const user = userEvent.setup();
  render(<Header canGrant />);
  await user.click(screen.getByRole("button", {name: FLEET.name}));
  await user.clear(field());
  await user.type(field(), `${RENAMED}{Enter}`);
  expect((await screen.findByRole("alert")).textContent).toBe(message);
  expect(field()).toHaveProperty("value", RENAMED);
  expect(network).toHaveBeenCalledTimes(1);
});

it("aborts a name change when the fleet page closes", async () => {
  const pending = Promise.withResolvers<Response>();
  network.mockReturnValueOnce(pending.promise);
  const user = userEvent.setup();
  const view = render(<Header canGrant />);
  await user.click(screen.getByRole("button", {name: FLEET.name}));
  await user.clear(field());
  await user.type(field(), `${RENAMED}{Enter}`);
  const signal = network.mock.calls[0]?.[1]?.signal;
  expect(signal?.aborted).toBe(false);
  view.unmount();
  expect(signal?.aborted).toBe(true);
  await act(async () => pending.resolve(response({...FLEET, name: RENAMED, etag: '"renamed"'})));
  expect(network).toHaveBeenCalledTimes(1);
});
