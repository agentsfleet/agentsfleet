import type { PlatformEntry } from "@/lib/api/platform-library-types";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import PlatformCatalogTable from "@/app/(dashboard)/admin/fleet-libraries/components/PlatformCatalogTable";
import FleetLibrariesView from "@/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView";
import { LIBRARY_CARD } from "./helpers/draft-fixtures";

const {patch, remove, refresh} = vi.hoisted(() => ({patch: vi.fn(), remove: vi.fn(), refresh: vi.fn()}));
vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({patchPlatformLibraryAction: patch, deletePlatformLibraryAction: remove}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh})}));
const ENTRY: PlatformEntry = {...LIBRARY_CARD, visibility: "platform_draft", published: false, updated_at: "1970-01-01T00:00:00Z"};
function show(entries = [ENTRY]) { return render(<TooltipProvider><FleetLibrariesView entries={entries} /></TooltipProvider>); }
beforeEach(() => vi.resetAllMocks());
afterEach(() => {cleanup(); vi.unstubAllGlobals(); vi.restoreAllMocks();});

describe("native platform Library curation", () => {
  it("sorts the catalog and keeps edited metadata after the editor closes", async () => {
    patch.mockResolvedValue({ok: true, data: {...ENTRY, name: "Edited name", etag: '"edited"'}});
    show();
    for (const name of ["Name", "Status", "Updated"]) fireEvent.click(screen.getByRole("button", {name: new RegExp(name)}));
    fireEvent.focus(screen.getByRole("button", {name: "Edit"}));
    fireEvent.pointerEnter(screen.getByRole("button", {name: "Edit"}));
    fireEvent.click(screen.getByRole("button", {name: "Edit"}));
    fireEvent.change(await screen.findByLabelText("Name"), {target: {value: "Edited name"}});
    fireEvent.click(screen.getByRole("button", {name: "Save"}));
    await screen.findByText("Edited name");
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(screen.getByRole("button", {name: "Edit"}));
    fireEvent.click(await screen.findByRole("button", {name: "Cancel"}));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
  it("retains a refused deletion and allows cancelling its confirmation", async () => {
    remove.mockRejectedValue(new Error("lost deletion answer"));
    show(); fireEvent.click(screen.getByRole("button", {name: "Delete"}));
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", {name: "Delete"}));
    await waitFor(() => expect(within(screen.getByRole("alertdialog")).getByRole("alert").textContent).toContain("Try again"));
    fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(screen.getByText(ENTRY.name)).toBeTruthy();
  });
  it("publishes only with the displayed tag and shows the server's new state", async () => {
    patch.mockResolvedValue({ok: true, data: {...ENTRY, visibility: "platform", published: true, etag: '\"published\"'}});
    show(); fireEvent.click(screen.getByRole("button", {name: "Publish"}));
    await waitFor(() => expect(screen.getByText("Published")).toBeTruthy());
    expect(screen.queryByRole("button", {name: "Delete"})).toBeNull();
    expect(patch).toHaveBeenCalledWith(ENTRY.id, {published: true}, ENTRY.etag);
    expect(screen.getByRole("button", {name: "Withdraw"})).toBeTruthy();
  });
  it("keeps a refused stale row unchanged and asks the server to refresh", async () => {
    patch.mockResolvedValue({ok: false, status: 412, error: "Changed by another operator"});
    show(); fireEvent.click(screen.getByRole("button", {name: "Publish"}));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("Changed by another operator"));
    expect(screen.getByText("Draft")).toBeTruthy(); expect(refresh).toHaveBeenCalledOnce(); expect(patch).toHaveBeenCalledOnce();
  });
  it("keeps an unknown mutation visible without guessing its success", async () => {
    patch.mockRejectedValue(new Error("Connection lost"));
    show(); fireEvent.click(screen.getByRole("button", {name: "Publish"}));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("Try again"));
    expect(screen.getByRole("alert").textContent).not.toContain("Connection lost");
    expect(screen.getByText("Draft")).toBeTruthy(); expect(refresh).toHaveBeenCalledOnce();
  });
  it("confirms withdrawal-only deletion against the tag captured when the dialog opened", async () => {
    remove.mockResolvedValue({ok: true, data: undefined});
    show(); fireEvent.click(screen.getByRole("button", {name: "Delete"}));
    expect(screen.getByRole("alertdialog").textContent).toContain("Existing fleets keep their saved setup");
    fireEvent.click(screen.getAllByRole("button", {name: "Delete"}).at(-1)!);
    await waitFor(() => expect(screen.getByText("No fleet setups yet")).toBeTruthy());
    expect(remove).toHaveBeenCalledWith(ENTRY.id, ENTRY.etag);
  });
  it("retains the entry after refused deletion", async () => {
    remove.mockResolvedValue({ok: false, status: 409, error: "Withdraw before deletion"});
    show(); fireEvent.click(screen.getByRole("button", {name: "Delete"}));
    fireEvent.click(screen.getAllByRole("button", {name: "Delete"}).at(-1)!);
    await waitFor(() => expect(within(screen.getByRole("alertdialog")).getByRole("alert").textContent).toContain("Withdraw before deletion"));
    expect(screen.getByText(ENTRY.name)).toBeTruthy(); expect(refresh).toHaveBeenCalledOnce();
  });
  it.each([false, true])("applies successive changes and deletion across server refresh=%s", async (serverRefresh) => {
    const published = {...ENTRY, visibility: "platform" as const, published: true, etag: '"published"'};
    const withdrawn = {...ENTRY, etag: '"withdrawn"'};
    patch.mockResolvedValueOnce({ok: true, data: published}).mockResolvedValueOnce({ok: true, data: withdrawn});
    remove.mockResolvedValue({ok: true, data: undefined});
    const view = show(); fireEvent.click(screen.getByRole("button", {name: "Publish"}));
    await screen.findByText("Published");
    if (serverRefresh) view.rerender(<TooltipProvider><PlatformCatalogTable entries={[published]} /></TooltipProvider>);
    fireEvent.click(screen.getByRole("button", {name: "Withdraw"}));
    await screen.findByText("Draft");
    fireEvent.click(screen.getByRole("button", {name: "Delete"}));
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", {name: "Delete"}));
    await screen.findByText("No fleet setups yet");
    expect(patch).toHaveBeenLastCalledWith(ENTRY.id, {published: false}, published.etag);
    expect(remove).toHaveBeenCalledWith(ENTRY.id, withdrawn.etag);
  });
  it("locks every row action while a publication is pending", async () => {
    let finish: ((value: unknown) => void) | undefined;
    patch.mockReturnValue(new Promise((resolve) => {finish = resolve;}));
    show(); fireEvent.click(screen.getByRole("button", {name: "Publish"}));
    for (const name of ["Edit", "Publish", "Delete"]) expect(screen.getByRole("button", {name}).hasAttribute("disabled")).toBe(true);
    await act(async () => {finish?.({ok: false, status: 412, error: "Refresh"});});
    expect(screen.getByRole("button", {name: "Publish"}).hasAttribute("disabled")).toBe(false);
  });
});


it("keeps pending deletion open until the server confirms its result", async () => {
  const pending = Promise.withResolvers<unknown>(); remove.mockReturnValueOnce(pending.promise);
  show(); fireEvent.click(screen.getByRole("button", {name: "Delete"}));
  fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", {name: "Delete"}));
  await userEvent.keyboard("{Escape}");
  expect(screen.getByRole("alertdialog")).toBeTruthy();
  await act(async () => pending.resolve({ok: false, status: 412, error: "Entry changed. Refresh before deleting."}));
  expect(within(screen.getByRole("alertdialog")).getByRole("alert").textContent).toContain("Entry changed");
});

it("opens editing deliberately on a device without hover", async () => {
  const matchMedia = window.matchMedia.bind(window);
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({...matchMedia(query), matches: query === "(pointer: coarse)"}));
  show(); fireEvent.pointerEnter(screen.getByRole("button", {name: "Edit"}));
  expect(screen.queryByRole("dialog")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Edit"}));
  expect(await screen.findByLabelText("Name")).toBeTruthy();
});

it("explains an empty platform library and provides a workspace destination", () => {
  show([]);
  expect(screen.getByText("Manage fleet setups shared across workspaces.")).toBeTruthy();
  expect(screen.getByText("No fleet setups yet")).toBeTruthy();
  expect(screen.getByRole("link", {name: "Open workspace"}).getAttribute("href")).toBe("/");
  expect(screen.queryByText(/catalog entries|reviewed candidate|stage a/i)).toBeNull();
});
