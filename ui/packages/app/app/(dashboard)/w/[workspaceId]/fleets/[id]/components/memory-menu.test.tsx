import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Button, DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@agentsfleet/design-system";
import { MemoryMenu } from "./MemoryMenu";

const {save, list, refresh} = vi.hoisted(() => ({save: vi.fn(), list: vi.fn(), refresh: vi.fn()}));
vi.mock("../../actions", () => ({setMemoryAccessAction: save, listFleetMemoriesAction: list, forgetMemoryAction: vi.fn()}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh})}));
beforeEach(() => {save.mockReset(); list.mockReset(); refresh.mockReset();});
afterEach(cleanup);
const READ = "Learn from this workspace";
const SHARE = "Share what this fleet learns";

function menu(canGrant = true, access = {read: false, publish: false}) {
  return <DropdownMenu><DropdownMenuTrigger asChild><Button>Fleet actions</Button></DropdownMenuTrigger><DropdownMenuContent>
    <MemoryMenu workspaceId="workspace" fleetId="fleet" access={access} canGrant={canGrant} />
  </DropdownMenuContent></DropdownMenu>;
}

it("places the workspace memory controls in the menu with plain descriptions", async () => {
  const user = userEvent.setup({delay: null});
  render(menu()); await user.click(screen.getByRole("button", {name: "Fleet actions"}));
  const read = screen.getByRole("menuitemcheckbox", {name: READ});
  expect(read.getAttribute("aria-checked")).toBe("false");
  expect(read.querySelector('[aria-hidden="true"][data-state="unchecked"]')).toBeTruthy();
  expect(document.getElementById(read.getAttribute("aria-describedby") ?? "")?.textContent).toBe("Use memories shared by other fleets here.");
  expect(screen.getByRole("separator")).toBeTruthy();
  expect(list.mock.calls).toEqual([]);
});

it("saves only the selected permission and refreshes the chat after its answer", async () => {
  save.mockResolvedValue({ok: true, data: {read: true, publish: false}});
  const user = userEvent.setup({delay: null});
  render(menu()); await user.click(screen.getByRole("button", {name: "Fleet actions"}));
  await user.click(screen.getByRole("menuitemcheckbox", {name: READ}));
  await waitFor(() => expect(screen.getByRole("menuitemcheckbox", {name: READ}).getAttribute("aria-checked")).toBe("true"));
  expect(screen.getByRole("menuitemcheckbox", {name: READ}).querySelector('[aria-hidden="true"][data-state="checked"]')).toBeTruthy();
  expect(screen.getByRole("menuitemcheckbox", {name: SHARE}).getAttribute("aria-checked")).toBe("false");
  expect(save).toHaveBeenCalledExactlyOnceWith("workspace", "fleet", {read: true});
  expect(refresh).toHaveBeenCalledOnce();
});

it("supports changing sharing with the keyboard while keeping the menu open", async () => {
  save.mockResolvedValue({ok: true, data: {read: false, publish: true}});
  const user = userEvent.setup({delay: null});
  render(menu()); await user.click(screen.getByRole("button", {name: "Fleet actions"}));
  screen.getByRole("menuitemcheckbox", {name: SHARE}).focus();
  await user.keyboard("{Enter}");
  await waitFor(() => expect(screen.getByRole("menuitemcheckbox", {name: SHARE}).getAttribute("aria-checked")).toBe("true"));
  expect(save).toHaveBeenCalledExactlyOnceWith("workspace", "fleet", {publish: true});
});

it.each([403, 503])("keeps the previous permission and shows a refused save (%i)", async status => {
  save.mockResolvedValue({ok: false, status, error: "Memory change refused"});
  const user = userEvent.setup({delay: null});
  render(menu()); await user.click(screen.getByRole("button", {name: "Fleet actions"}));
  await user.click(screen.getByRole("menuitemcheckbox", {name: READ}));
  expect((await screen.findByRole("alert")).textContent).toContain("Memory change refused");
  expect(screen.getByRole("menuitemcheckbox", {name: READ}).getAttribute("aria-checked")).toBe("false");
  expect(refresh).not.toHaveBeenCalled();
});

it("permits retry after transport loss and blocks overlapping saves", async () => {
  const pending = Promise.withResolvers<unknown>();
  save.mockReturnValueOnce(pending.promise).mockResolvedValueOnce({ok: true, data: {read: false, publish: true}});
  const user = userEvent.setup({delay: null});
  render(menu()); await user.click(screen.getByRole("button", {name: "Fleet actions"}));
  await user.click(screen.getByRole("menuitemcheckbox", {name: READ}));
  expect(screen.getByRole("menuitemcheckbox", {name: SHARE}).getAttribute("aria-disabled")).toBe("true");
  await user.click(screen.getByRole("menuitemcheckbox", {name: SHARE}));
  expect(save.mock.calls).toHaveLength(1);
  pending.reject(new Error("Connection lost"));
  await screen.findByRole("alert");
  await user.click(screen.getByRole("menuitemcheckbox", {name: SHARE}));
  await waitFor(() => expect(screen.getByRole("menuitemcheckbox", {name: SHARE}).getAttribute("aria-checked")).toBe("true"));
});

// Value: protects=one memory grant write during rapid input and background refresh; fails_when=duplicate events write twice or props replace pending state;
// why_new=the overlap test waits for the disabled state first; seam=none; no_claim=backend exactly-once writes.
it("keeps the observed permission until its single pending change is confirmed", async () => {
  const pending = Promise.withResolvers<unknown>();
  save.mockReturnValueOnce(pending.promise);
  const user = userEvent.setup({delay: null});
  const view = render(menu());
  await user.click(screen.getByRole("button", {name: "Fleet actions"}));
  const read = screen.getByRole("menuitemcheckbox", {name: READ});
  act(() => {read.click(); read.click();});
  view.rerender(menu(true, {read: true, publish: true}));
  expect(read.getAttribute("aria-checked")).toBe("false");
  expect(screen.getByRole("menuitemcheckbox", {name: SHARE}).getAttribute("aria-checked")).toBe("false");
  expect(save).toHaveBeenCalledExactlyOnceWith("workspace", "fleet", {read: true});
  await act(async () => pending.resolve({ok: true, data: {read: true, publish: false}}));
  expect(read.getAttribute("aria-checked")).toBe("true");
  expect(screen.getByRole("menuitemcheckbox", {name: SHARE}).getAttribute("aria-checked")).toBe("false");
  expect(refresh).toHaveBeenCalledOnce();
});

it("lets a viewer open saved memories without exposing permission changes", async () => {
  list.mockResolvedValue({ok: true, data: {items: [], next_cursor: null}});
  const user = userEvent.setup({delay: null});
  render(menu(false)); await user.click(screen.getByRole("button", {name: "Fleet actions"}));
  expect(screen.queryByRole("menuitemcheckbox")).toBeNull();
  await user.click(screen.getByRole("menuitem", {name: "Saved memories"}));
  expect(await screen.findByRole("dialog", {name: "Saved memories"})).toBeTruthy();
  expect(await screen.findByText("Nothing learned yet")).toBeTruthy();
  expect(screen.queryByRole("heading", {name: /^Memory$/})).toBeNull();
  expect(save.mock.calls).toEqual([]);
});

it("keeps memory loading failure visible and retries without closing the conversation", async () => {
  list.mockResolvedValueOnce({ok: false, error: "Memories unavailable"}).mockResolvedValueOnce({ok: true, data: {items: [], next_cursor: null}});
  const user = userEvent.setup({delay: null});
  render(menu()); await user.click(screen.getByRole("button", {name: "Fleet actions"}));
  await user.click(screen.getByRole("menuitem", {name: "Saved memories"}));
  expect((await screen.findByRole("alert")).textContent).toContain("Memories unavailable");
  await user.click(screen.getByRole("button", {name: "Try again"}));
  expect(await screen.findByText("Nothing learned yet")).toBeTruthy();
});

// Value: protects=retry after a rejected memories request; fails_when=transport loss becomes an empty-memory claim;
// why_new=the existing retry test covered a returned refusal only; seam=none; no_claim=memory persistence.
it("shows transport loss as a loading error and permits retry in the same dialog", async () => {
  list.mockRejectedValueOnce(new TypeError("Connection lost"))
    .mockResolvedValueOnce({ok: true, data: {items: [], next_cursor: null}});
  const user = userEvent.setup({delay: null});
  render(menu()); await user.click(screen.getByRole("button", {name: "Fleet actions"}));
  await user.click(screen.getByRole("menuitem", {name: "Saved memories"}));
  expect((await screen.findByRole("alert")).textContent).toContain("Saved memories could not be loaded.");
  expect(screen.queryByText("Nothing learned yet")).toBeNull();
  const dialog = screen.getByRole("dialog");
  await user.click(screen.getByRole("button", {name: "Try again"}));
  expect(await screen.findByText("Nothing learned yet")).toBeTruthy();
  expect(screen.getByRole("dialog")).toBe(dialog);
  expect(screen.queryByRole("alert")).toBeNull();
});

// Value: protects=closed-dialog response isolation; fails_when=an old success or rejection contaminates a reopened browser;
// why_new=no existing request completed after its browser unmounted; seam=none; no_claim=request cancellation.
it.each(["resolve", "reject"])("ignores a previous memories request after closing and reopening (%s)", async completion => {
  const pending = Promise.withResolvers<unknown>();
  list.mockReturnValueOnce(pending.promise).mockResolvedValueOnce({ok: true, data: {items: [], next_cursor: null}});
  const user = userEvent.setup({delay: null});
  render(menu()); await user.click(screen.getByRole("button", {name: "Fleet actions"}));
  await user.click(screen.getByRole("menuitem", {name: "Saved memories"}));
  expect(await screen.findByText("Loading memories…")).toBeTruthy();
  await user.click(screen.getByRole("button", {name: "Close"}));
  await user.click(screen.getByRole("menuitem", {name: "Saved memories"}));
  expect(await screen.findByText("Nothing learned yet")).toBeTruthy();
  await act(async () => {
    if (completion === "reject") pending.reject(new TypeError("Old connection failed"));
    else pending.resolve({ok: false, error: "Old memories unavailable"});
  });
  expect(screen.getByText("Nothing learned yet")).toBeTruthy();
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.queryByText("Loading memories…")).toBeNull();
});
