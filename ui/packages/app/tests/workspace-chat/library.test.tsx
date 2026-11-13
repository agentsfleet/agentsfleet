import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { LibraryBrowser } from "@/components/domain/chat/LibraryBrowser";
import { LibraryWorkspace } from "@/components/domain/chat/LibraryWorkspace";
import { SOURCE_KIND } from "@/lib/api/draft-vocabulary";
import { FLEET_ID, LIBRARY, LIBRARY_CARD, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";

const { removeEntry, push, identity } = vi.hoisted(() => ({ removeEntry: vi.fn(), push: vi.fn(),
  identity: {isLoaded: true, isSignedIn: true, userId: "plugin-owner"} }));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => identity}));
vi.mock("next/navigation", () => ({useRouter: () => ({push})}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/library/actions", () => ({ removeLibraryEntryAction: removeEntry }));

const PROPS = {workspace: WORKSPACE, onSelect: vi.fn()};
const EMPTY_HISTORY = {items: [], next_cursor: null};
afterEach(() => { cleanup(); vi.unstubAllGlobals(); vi.clearAllMocks(); Object.assign(identity, {isLoaded: true, isSignedIn: true, userId: "plugin-owner"}); });

it("waits for the signed-in account before reading its plugins", async () => {
  identity.isLoaded = false;
  const fetcher = vi.fn(async () => response({items: [], next_cursor: null}));
  vi.stubGlobal("fetch", fetcher);
  const view = render(<LibraryBrowser {...PROPS} />);
  expect(screen.getByText("Loading account")).toBeTruthy();
  expect(fetcher).not.toHaveBeenCalled();
  identity.isLoaded = true; identity.isSignedIn = false;
  view.rerender(<LibraryBrowser {...PROPS} />);
  expect(screen.queryByText("Loading account")).toBeNull();
  expect(fetcher).not.toHaveBeenCalled();
  identity.isSignedIn = true;
  view.rerender(<LibraryBrowser {...PROPS} />);
  await screen.findByText(/No published setups are available yet/);
  expect(fetcher).toHaveBeenCalledOnce();
});

it("edits an owned plugin at its inspected revision without creating a fleet", async () => {
  const card = {...LIBRARY_CARD, visibility: "tenant" as const};
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const path = new URL(input, "https://app.test").pathname;
    return response(path.endsWith("/owned") ? {items: [card], next_cursor: null}
      : path.endsWith("/revisions") ? EMPTY_HISTORY : {...LIBRARY, card});
  }));
  render(<LibraryWorkspace workspace={WORKSPACE} />);
  fireEvent.click(await screen.findByRole("button", {name: `Manage ${card.name}`}));
  fireEvent.click(await screen.findByRole("button", {name: "Edit setup"}));
  expect(push).toHaveBeenCalledWith(`/w/${WORKSPACE}/fleets/new?library=${card.id}&revision=${LIBRARY.head.id}&intent=edit`);
});

it("pages the owned plugin list and returns to its first page", async () => {
  const fetcher = vi.fn(async (input: string) => {
    const url = new URL(input, "https://app.test");
    const later = url.searchParams.has("starting_after");
    return response({items: later ? [] : [LIBRARY_CARD], next_cursor: later ? null : "later"});
  });
  vi.stubGlobal("fetch", fetcher);
  render(<LibraryBrowser {...PROPS} />);
  await screen.findByText(LIBRARY_CARD.name);
  expect(screen.queryByRole("tab")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Next page"}));
  await screen.findByText("No published setups on this page. Check another page.");
  fireEvent.click(screen.getByRole("button", {name: "First page"}));
  await screen.findByText(LIBRARY_CARD.name);
  expect(fetcher.mock.calls.every(([input]) => new URL(input, "https://app.test").pathname.endsWith("/owned"))).toBe(true);
  const last = fetcher.mock.calls.at(-1)?.[0];
  expect(last && new URL(last, "https://app.test").searchParams.has("starting_after")).toBe(false);
});

it("opens a new private chat pinned to the inspected Library revision", async () => {
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const path = new URL(input, "https://app.test").pathname;
    return response(path.endsWith("/owned") ? {items: [LIBRARY_CARD], next_cursor: null}
      : path.endsWith("/revisions") ? EMPTY_HISTORY : LIBRARY);
  }));
  render(<LibraryWorkspace workspace={WORKSPACE} />);
  fireEvent.click(await screen.findByRole("button", {name: `Manage ${LIBRARY_CARD.name}`}));
  fireEvent.click(await screen.findByRole("button", {name: "Use for new fleet"}));
  expect(push).toHaveBeenCalledWith(`/w/${WORKSPACE}/fleets/new?library=${LIBRARY_CARD.id}&revision=${LIBRARY.head.id}&intent=new`);
});

it("shows loading until the list proves it is empty", async () => {
  const list = Promise.withResolvers<Response>();
  vi.stubGlobal("fetch", vi.fn(() => list.promise));
  render(<LibraryBrowser {...PROPS} />);
  expect(screen.getByText("Loading published setups")).not.toBeNull();
  expect(screen.queryByText(/No (published setups are available yet|setups have been published by this workspace)/)).toBeNull();
  await act(async () => list.resolve(response({items: [], next_cursor: null})));
  await screen.findByText(/No (published setups are available yet|setups have been published by this workspace)/);
  expect(screen.queryByText("Loading published setups")).toBeNull();
});

it("recovers a failed list without closing or reopening the Library", async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(response({detail: "Unavailable"}, 503))
    .mockResolvedValueOnce(response({items: [LIBRARY_CARD], next_cursor: null}));
  vi.stubGlobal("fetch", fetcher);
  render(<LibraryBrowser {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Retry plugins"}));
  await screen.findByText(LIBRARY_CARD.name);
  expect(fetcher).toHaveBeenCalledTimes(2); expect(screen.queryByRole("alert")).toBeNull();
});

it.each([404, 503, 200])("keeps a refused or incompatible library read separate from empty results (%i)", async (status) => {
  const fetcher = vi.fn().mockResolvedValueOnce(response({entries: []}, status))
    .mockResolvedValueOnce(response({items: [LIBRARY_CARD], next_cursor: null}));
  vi.stubGlobal("fetch", fetcher);
  render(<LibraryBrowser {...PROPS} />);
  await screen.findByText("Plugins are unavailable right now");
  expect(screen.getByRole("alert").getAttribute("data-slot")).toBe("unavailable-state");
  expect(screen.queryByText(/No (published setups are available yet|setups have been published by this workspace)/)).toBeNull();
  expect(screen.queryByText(/This published setup is no longer available/)).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Retry plugins"}));
  await screen.findByText(LIBRARY_CARD.name);
  expect(screen.queryByRole("alert")).toBeNull();
});

it("removes stale actions while inspecting history and uses the selected immutable revision", async () => {
  const historical = Promise.withResolvers<Response>();
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const url = new URL(input, "https://app.test");
    if (url.pathname.endsWith("/owned")) return response({items: [LIBRARY_CARD], next_cursor: null});
    if (url.pathname.endsWith("/revisions")) return response({items: [{id: FLEET_ID, revision: "0", canonical_digest: "older", created_at: "1970-01-01T00:00:00Z"}], next_cursor: null});
    if (url.searchParams.get("revision_id") === FLEET_ID) return historical.promise;
    return response(LIBRARY);
  }));
  render(<LibraryBrowser {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: `Manage ${LIBRARY_CARD.name}`}));
  await screen.findByRole("button", {name: "Use for new fleet"});
  fireEvent.click(screen.getByRole("button", {name: "Choose a saved version"}));
  fireEvent.click(await screen.findByRole("button", {name: "Inspect revision 0"}));
  expect(screen.queryByRole("button", {name: "Use for new fleet"})).toBeNull();
  expect(screen.getByText("Loading selected revision")).not.toBeNull();
  await act(async () => historical.resolve(response({...LIBRARY, head: {...LIBRARY.head, id: FLEET_ID, revision: "0"}})));
  fireEvent.click(await screen.findByRole("button", {name: "Use for new fleet"}));
  expect(PROPS.onSelect).toHaveBeenCalledWith({kind: SOURCE_KIND.library, library_id: LIBRARY_CARD.id, revision_id: FLEET_ID}, "new");
});

it("aborts inspection on close and ignores its delayed response", async () => {
  const inspection = Promise.withResolvers<Response>();
  let signal: AbortSignal | undefined;
  vi.stubGlobal("fetch", vi.fn(async (input: string, init: RequestInit) => {
    const path = new URL(input, "https://app.test").pathname;
    if (path.endsWith("/owned")) return response({items: [LIBRARY_CARD], next_cursor: null});
    if (path.endsWith("/revisions")) return response(EMPTY_HISTORY);
    signal = init.signal ?? undefined; return inspection.promise;
  }));
  const view = render(<LibraryBrowser {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: `Manage ${LIBRARY_CARD.name}`}));
  await waitFor(() => expect(signal).toBeDefined());
  view.unmount();
  expect(signal?.aborted).toBe(true);
  await act(async () => inspection.resolve(response(LIBRARY)));
  expect(screen.queryByRole("button", {name: "Use for new fleet"})).toBeNull();
  expect(PROPS.onSelect).not.toHaveBeenCalled();
});

it.each([200, 503])("can inspect the same recipe again after its first answer is %i", async (status) => {
  let inspections = 0;
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const path = new URL(input, "https://app.test").pathname;
    if (path.endsWith("/owned")) return response({items: [LIBRARY_CARD], next_cursor: null});
    if (path.endsWith("/revisions")) return response(EMPTY_HISTORY);
    inspections += 1;
    return response(LIBRARY, inspections === 1 ? status : 200);
  }));
  render(<LibraryBrowser {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: `Manage ${LIBRARY_CARD.name}`}));
  if (status === 503) await screen.findByRole("alert");
  else await screen.findByRole("button", {name: "Use for new fleet"});
  if (status === 503) fireEvent.click(screen.getByRole("button", {name: "Retry plugin"}));
  else {
    fireEvent.click(screen.getByRole("button", {name: "Close"}));
    fireEvent.click(screen.getByRole("button", {name: `Manage ${LIBRARY_CARD.name}`}));
  }
  await screen.findByRole("button", {name: "Use for new fleet"});
  expect(screen.queryByText("Loading selected revision")).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("confirms owned recipe removal and keeps a refused recipe visible", async () => {
  const pending = Promise.withResolvers<{ ok: boolean; error: string; status: number }>();
  removeEntry.mockReturnValue(pending.promise);
  vi.stubGlobal("fetch", vi.fn(async () => response({items: [{...LIBRARY_CARD, visibility: "tenant"}], next_cursor: null})));
  render(<LibraryBrowser {...PROPS} />);
  await screen.findByText(LIBRARY_CARD.name);
  expect(screen.queryByRole("button", {name: "Remove from this workspace"})).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: `Manage ${LIBRARY_CARD.name}`}));
  fireEvent.click(await screen.findByRole("button", {name: "Remove from this workspace"}));
  expect(removeEntry).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", {name: "Remove"}));
  expect(screen.getByRole("button", {name: "Working…"}).hasAttribute("disabled")).toBe(true);
  expect(removeEntry).toHaveBeenCalledOnce();
  expect(removeEntry).toHaveBeenCalledWith(WORKSPACE, LIBRARY_CARD.id);
  await act(async () => pending.resolve({ok: false, error: "Not permitted", status: 403}));
  expect(screen.getByRole("alert").textContent).toContain("Not permitted");
  expect(screen.getAllByText(LIBRARY_CARD.name).length).toBeGreaterThan(0);
});

it("can page recipes and revision history and inspect the same historical revision again", async () => {
  const paths: string[] = [];
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    paths.push(input); const url = new URL(input, "https://app.test");
    if (url.pathname.endsWith("/owned")) return response({items: [LIBRARY_CARD], next_cursor: url.searchParams.has("starting_after") ? null : "page-two"});
    if (url.pathname.endsWith("/revisions")) return response({items: [{id: FLEET_ID, revision: "0", canonical_digest: "older", created_at: "1970-01-01T00:00:00Z"}], next_cursor: url.searchParams.has("starting_after") ? null : "older"});
    return response({...LIBRARY, head: {...LIBRARY.head, id: url.searchParams.get("revision_id")}});
  }));
  render(<LibraryBrowser {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Next page"}));
  await waitFor(() => expect(screen.queryByRole("button", {name: "Next page"})).toBeNull());
  fireEvent.click(screen.getByRole("button", {name: `Manage ${LIBRARY_CARD.name}`}));
  fireEvent.click(screen.getByRole("button", {name: "Choose a saved version"}));
  fireEvent.click(await screen.findByRole("button", {name: "Older revisions"}));
  await waitFor(() => expect(screen.queryByRole("button", {name: "Older revisions"})).toBeNull());
  for (let attempt = 0; attempt < 2; attempt += 1) {
    fireEvent.click(screen.getByRole("button", {name: "Inspect revision 0"}));
    await screen.findByRole("button", {name: "Use for new fleet"});
  }
  expect(paths.some((path) => path.includes("starting_after=page-two"))).toBe(true);
  expect(paths.some((path) => path.includes("starting_after=older"))).toBe(true);
});

it.each([403, 503])("handles revision history refusal %i with the correct recovery scope", async (status) => {
  let reads = 0;
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const path = new URL(input, "https://app.test").pathname;
    if (path.endsWith("/owned")) return response({items: [LIBRARY_CARD], next_cursor: null});
    if (path.endsWith("/revisions")) { reads += 1; return response(EMPTY_HISTORY, reads === 1 ? status : 200); }
    return response(LIBRARY);
  }));
  render(<LibraryBrowser {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: `Manage ${LIBRARY_CARD.name}`}));
  fireEvent.click(screen.getByRole("button", {name: "Choose a saved version"}));
  const alert = await screen.findByRole("alert");
  expect(alert.getAttribute("data-slot")).toBe("unavailable-state");
  if (status === 403) {
    expect(screen.queryByRole("button", {name: "Use for new fleet"})).toBeNull();
    fireEvent.click(screen.getByRole("button", {name: "Retry plugins"}));
    await screen.findByText(LIBRARY_CARD.name);
  } else {
    fireEvent.click(screen.getByRole("button", {name: "Retry revision history"}));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    expect(reads).toBe(2);
  }
});

it("retains a removal failure inside the dialog and removes the row after successful retry", async () => {
  let removed = false;
  removeEntry.mockRejectedValueOnce(new Error("lost answer")).mockImplementationOnce(async () => {removed = true; return {ok: true};});
  vi.stubGlobal("fetch", vi.fn(async () => response({items: removed ? [] : [{...LIBRARY_CARD, visibility: "tenant"}], next_cursor: null})));
  render(<LibraryBrowser {...PROPS} />);
  await screen.findByText(LIBRARY_CARD.name);
  fireEvent.click(screen.getByRole("button", {name: `Manage ${LIBRARY_CARD.name}`}));
  fireEvent.click(await screen.findByRole("button", {name: "Remove from this workspace"}));
  fireEvent.click(screen.getByRole("button", {name: "Remove"}));
  await screen.findByText("Removal could not be confirmed. Refresh the Library before trying again.");
  fireEvent.click(screen.getByRole("button", {name: "Remove"}));
  await screen.findByText(/No (published setups are available yet|setups have been published by this workspace)/);
  expect(screen.queryByRole("alertdialog")).toBeNull();

});


it("offers a new fleet from an empty owned plugin list", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => response(EMPTY_HISTORY)));
  render(<LibraryBrowser {...PROPS} />);
  expect(screen.getByText("Current account")).toBeTruthy();
  const link = await screen.findByRole("link", {name: "New fleet"});
  expect(link.getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/new`);
});
