import {useState} from "react";
import {act, cleanup, fireEvent, render, screen, waitFor} from "@testing-library/react";
import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {PluginPublicationTarget} from "@/components/domain/chat/PluginPublicationTarget";
import {LIBRARY_CARD, WORKSPACE} from "./helpers/draft-fixtures";
import type {libraryPage} from "@/lib/api/draft-library";

const {read, change} = vi.hoisted(() => ({read: vi.fn<typeof libraryPage>(), change: vi.fn()}));
vi.mock("@/lib/api/draft-library", () => ({libraryPage: read}));
const own = {...LIBRARY_CARD, visibility: "tenant" as const};
const page = {items: [own], next_cursor: null};
beforeEach(() => {read.mockReset(); change.mockReset();});
afterEach(cleanup);
function Picker({workspace = WORKSPACE}: {workspace?: string}) {
  const [value, setValue] = useState<string>();
  return <PluginPublicationTarget workspace={workspace} disabled={false} value={value}
    onChange={next => {setValue(next); change(next);}} />;
}

// Value: protects=explicit owned-plugin selection across paging and failed reads;
// fails_when=a page switch silently retains a publication destination; seam=library reader; no_claim=publication writes.
it("filters public entries, selects explicitly and clears the destination when paging", async () => {
  read.mockResolvedValueOnce({items: [own, {...LIBRARY_CARD, id: "public", name: "Public recipe", visibility: "platform"}], next_cursor: "next"})
    .mockResolvedValueOnce({items: [], next_cursor: null}).mockResolvedValueOnce(page);
  render(<Picker />);
  const select = await screen.findByRole("combobox");
  expect(change).not.toHaveBeenCalled();
  fireEvent.click(select);
  fireEvent.keyDown(select, {key: "Enter"});
  expect(screen.queryByRole("option", {name: "Public recipe"})).toBeNull();
  fireEvent.click(await screen.findByRole("option", {name: own.name}));
  expect(change).toHaveBeenLastCalledWith(own.id);
  fireEvent.click(screen.getByRole("button", {name: "Next page"}));
  await screen.findByText("No plugins on this page.");
  expect(change).toHaveBeenLastCalledWith(undefined);
  expect(read).toHaveBeenLastCalledWith(WORKSPACE, true, "next", expect.any(AbortSignal));
  fireEvent.click(screen.getByRole("button", {name: "First page"}));
  await screen.findByRole("combobox");
  expect(read).toHaveBeenLastCalledWith(WORKSPACE, true, null, expect.any(AbortSignal));
});

it("makes a refused read retryable without selecting or writing a plugin", async () => {
  read.mockRejectedValueOnce(new Error("Disconnected")).mockResolvedValueOnce(page);
  render(<Picker />);
  fireEvent.click(await screen.findByRole("button", {name: "Try again"}));
  await screen.findByRole("combobox");
  expect(screen.queryByRole("alert")).toBeNull();
  expect(read).toHaveBeenCalledTimes(2);
  expect(change).not.toHaveBeenCalled();
});

it.each([false, true])("ignores a stale account's completed or rejected read (%s)", async reject => {
  const pending = Promise.withResolvers<Awaited<ReturnType<typeof libraryPage>>>();
  read.mockReturnValueOnce(pending.promise).mockResolvedValueOnce({items: [], next_cursor: null});
  const view = render(<Picker />);
  const oldSignal = read.mock.calls[0]?.[3];
  view.rerender(<Picker workspace="another-workspace" />);
  await screen.findByText("No plugins on this page.");
  expect(oldSignal?.aborted).toBe(true);
  await act(async () => {if (reject) pending.reject(new Error("Late failure")); else pending.resolve(page);});
  await waitFor(() => expect(screen.queryByRole("combobox")).toBeNull());
  expect(screen.queryByRole("alert")).toBeNull();
  expect(change).not.toHaveBeenCalled();
});
