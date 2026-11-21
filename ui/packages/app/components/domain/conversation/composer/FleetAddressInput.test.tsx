import type { AppendMessage, ThreadMessageLike } from "@assistant-ui/react";
import { useRef } from "react";
import { AssistantRuntimeProvider, ComposerPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FleetAddressInput } from "./FleetAddressInput";
import { FLEET_DETAIL, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";
import userEvent from "@testing-library/user-event";

const location = vi.hoisted(() => ({path: ""}));
vi.mock("next/navigation", () => ({usePathname: () => location.path}));
const OTHER = "0199a801-1000-7000-8000-000000000008";
const options = [
  {...FLEET_DETAIL, description: "Handoff", name: "bob", status: "active"},
  {...FLEET_DETAIL, description: "Handoff", id: OTHER, name: "bobby", status: "active"},
  {...FLEET_DETAIL, description: "Handoff", id: "0199a801-1000-7000-8000-000000000009", name: "bob-archived", status: "archived"},
];
const read = vi.fn();
const page = (items = options, cursor: string | null = null) => response({items, next_cursor: cursor, total: null});

function Harness({disabled = false, onNew = async () => {}}: {disabled?: boolean; onNew?: (message: AppendMessage) => Promise<void>}) {
  const runtime = useExternalStoreRuntime({messages: [], convertMessage: (message: ThreadMessageLike) => message, onNew});
  const ref = useRef<HTMLTextAreaElement>(null);
  return <AssistantRuntimeProvider runtime={runtime}><ComposerPrimitive.Root>
    <FleetAddressInput inputRef={ref} placeholder="Message" disabled={disabled} />
  </ComposerPrimitive.Root></AssistantRuntimeProvider>;
}
function enter(text: string) {
  const input = screen.getByRole<HTMLTextAreaElement>("textbox", {name: "Message"});
  act(() => input.focus());
  fireEvent.change(input, {target: {value: text}});
  return input;
}
beforeEach(() => {
  location.path = `/w/${WORKSPACE}/fleets`;
  read.mockReset().mockImplementation(async () => page());
  vi.stubGlobal("fetch", read);
  vi.spyOn(HTMLElement.prototype, "scrollIntoView").mockImplementation(() => {});
});
afterEach(() => {cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals();});

it("filters active workspace fleets locally and selects using arrow keys", async () => {
  render(<Harness />);
  const input = enter("@");
  await screen.findByRole("option", {name: "@bob"});
  enter("@bo");
  expect(screen.queryByText("@bob-archived")).toBeNull();
  expect(read).toHaveBeenCalledTimes(1);
  fireEvent.keyDown(input, {key: "ArrowUp"});
  fireEvent.keyDown(input, {key: "ArrowDown"});
  fireEvent.keyDown(input, {key: "ArrowDown"});
  fireEvent.keyDown(input, {key: "Enter", shiftKey: true});
  fireEvent.keyDown(input, {key: "Enter", isComposing: true});
  expect(input.value).toBe("@bo");
  fireEvent.keyDown(input, {key: "Enter"});
  expect(input.value).toBe("@bobby ");
  expect(screen.queryByRole("listbox")).toBeNull();
  input.setSelectionRange(2, 2);
  fireEvent.select(input);
  expect(screen.queryByRole("listbox")).toBeNull();
  enter("@bobby ask @bo");
  expect(screen.queryByRole("listbox")).toBeNull();
});

it("preserves words on both sides of a selected mention", async () => {
  render(<Harness />);
  const input = enter("Check @bo tomorrow");
  input.setSelectionRange(9, 9);
  fireEvent.select(input);
  const bob = await screen.findByRole("option", {name: "@bob"});
  await userEvent.click(bob);
  expect(input.value).toBe("@bob Check tomorrow");
  expect(document.activeElement).toBe(input);
  expect(screen.queryByRole("listbox")).toBeNull();
});

it("lets Enter submit normally when the open picker has no matching choice", async () => {
  const send = vi.fn<(message: AppendMessage) => Promise<void>>(async () => {});
  render(<Harness onNew={send} />);
  const input = enter("@unknown");
  await screen.findByText("No matching fleets.");
  fireEvent.keyDown(input, {key: "Enter"});
  await waitFor(() => expect(send).toHaveBeenCalledOnce());
  expect(send.mock.calls[0]?.[0]).toMatchObject({content: [{type: "text", text: "@unknown"}]});
  expect(document.activeElement).toBe(input);
});

it("handles dismissal, empty results, paging and retry inside the popup", async () => {
  read.mockImplementationOnce(async () => response({}, 503))
    .mockImplementationOnce(async () => page([], "older"));
  render(<Harness />);
  const input = enter("@bo");
  const retry = await screen.findByRole("button", {name: "Retry"});
  act(() => retry.focus());
  fireEvent.keyDown(retry, {key: "Enter"});
  fireEvent.click(retry);
  await screen.findByText("No matches on this page.");
  act(() => input.focus());
  fireEvent.keyDown(input, {key: "ArrowDown"});
  expect(input.value).toBe("@bo");
  fireEvent.click(screen.getByRole("button", {name: "Load more fleets"}));
  await screen.findByRole("option", {name: "@bob"});
  const list = screen.getByRole("listbox");
  fireEvent.click(list);
  fireEvent.change(list, {target: {value: "bob"}});
  expect(input.value).toBe("@bob ");
  enter("@nobody");
  await screen.findByText("No matching fleets.");
  fireEvent.keyDown(input, {key: "Escape"});
  expect(screen.queryByText("No matching fleets.")).toBeNull();
  enter("@b");
  await screen.findByRole("option", {name: "@bob"});
  act(() => input.blur());
  expect(screen.queryByRole("listbox")).toBeNull();
});

it("never displays a late list from another workspace", async () => {
  const late = Promise.withResolvers<Response>();
  read.mockImplementationOnce(() => late.promise)
    .mockImplementationOnce(async () => page([{...options[0]!, name: "alice"}]));
  const view = render(<Harness />);
  const input = enter("@");
  await screen.findByText("Loading fleets…");
  location.path = `/w/${OTHER}/fleets`;
  view.rerender(<Harness />);
  await screen.findByRole("option", {name: "@alice"});
  await act(async () => late.resolve(page()));
  expect(screen.queryByText("@bob")).toBeNull();
  expect(read.mock.calls.map(([url]) => url)).toEqual([
    `/live/v1/workspaces/${WORKSPACE}/fleets?limit=50`, `/live/v1/workspaces/${OTHER}/fleets?limit=50`,
  ]);
  view.rerender(<Harness disabled />);
  expect(screen.queryByRole("listbox")).toBeNull();
  location.path = "/";
  view.rerender(<Harness />);
  fireEvent.keyDown(input, {key: "ArrowDown"});
  await waitFor(() => expect(read).toHaveBeenCalledTimes(2));
});
