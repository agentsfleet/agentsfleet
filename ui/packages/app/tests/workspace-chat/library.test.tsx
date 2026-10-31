import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { LibraryBrowser } from "@/components/domain/chat/LibraryBrowser";
import { LibraryWorkspace } from "@/components/domain/chat/LibraryWorkspace";
import { SOURCE_KIND } from "@/lib/api/draft-vocabulary";
import { FLEET_ID, LIBRARY, LIBRARY_CARD, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";

const { removeEntry, push } = vi.hoisted(() => ({ removeEntry: vi.fn(), push: vi.fn() }));
vi.mock("next/navigation", () => ({useRouter: () => ({push})}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/library/actions", () => ({ removeLibraryEntryAction: removeEntry }));

const PROPS = {workspace: WORKSPACE, onSelect: vi.fn()};
const EMPTY_HISTORY = {items: [], next_cursor: null};
afterEach(() => { cleanup(); vi.unstubAllGlobals(); vi.clearAllMocks(); });

it.each([false, true])("returns to the first page without changing library scope (owned=%s)", async (owned) => {
  const firstPageLabel = "First page";
  const nextPageLabel = "Next page";
  const fetcher = vi.fn(async (input: string) => {
    const url = new URL(input, "https://app.test");
    const later = url.searchParams.has("starting_after");
    return response({items: later ? [] : [LIBRARY_CARD], next_cursor: later ? null : "later"});
  });
  vi.stubGlobal("fetch", fetcher);
  render(<LibraryBrowser {...PROPS} />);
  const scope = screen.getByRole("tab", {name: owned ? "Workspace" : "All"});
  if (owned) fireEvent.mouseDown(scope);
  await screen.findByText(LIBRARY_CARD.name);
  expect(screen.queryByRole("button", {name: firstPageLabel})).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: nextPageLabel}));
  await waitFor(() => expect(screen.queryByText(LIBRARY_CARD.name)).toBeNull());
  fireEvent.click(screen.getByRole("button", {name: firstPageLabel}));
  await screen.findByText(LIBRARY_CARD.name);
  const last = fetcher.mock.calls.at(-1)?.[0];
  if (!last) throw new Error("Expected a library page request");
  const url = new URL(last, "https://app.test");
  expect(url.pathname.endsWith("/owned")).toBe(owned);
  expect(url.searchParams.has("starting_after")).toBe(false);
  expect(scope.getAttribute("aria-selected")).toBe("true");
  expect(screen.queryByRole("button", {name: firstPageLabel})).toBeNull();
  expect(screen.getByRole("button", {name: nextPageLabel})).toBeTruthy();
});

it("keeps empty later pages distinct and resets the cursor when changing tabs", async () => {
  const fetcher = vi.fn(async (input: string) => {
    const url = new URL(input, "https://app.test");
    const owned = url.pathname.endsWith("/owned");
    return response({ items: [], next_cursor: owned || url.searchParams.has("starting_after") ? null : "later" });
  });
  vi.stubGlobal("fetch", fetcher);
  render(<LibraryBrowser {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Next page"}));
  await waitFor(() => expect(fetcher.mock.calls.some(([input]) => new URL(input, "https://app.test").searchParams.get("starting_after") === "later")).toBe(true));
  await screen.findByText("No published setups on this page. Check another page or change the library scope.");
  fireEvent.mouseDown(screen.getByRole("tab", {name: "Workspace"}));
  await screen.findByText(/No setups have been published by this workspace/);
  expect(screen.getByRole("tab", {name: "Workspace"}).getAttribute("aria-selected")).toBe("true");
  const last = fetcher.mock.calls.at(-1)?.[0];
  expect(last && new URL(last, "https://app.test").searchParams.has("starting_after")).toBe(false);
});

it("opens a new private chat pinned to the inspected Library revision", async () => {
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const path = new URL(input, "https://app.test").pathname;
    return response(path.endsWith("/fleet-libraries") ? {items: [LIBRARY_CARD], next_cursor: null}
      : path.endsWith("/revisions") ? EMPTY_HISTORY : LIBRARY);
  }));
  render(<LibraryWorkspace workspace={WORKSPACE} />);
  fireEvent.click(await screen.findByRole("button", {name: "Inspect"}));
  fireEvent.click(await screen.findByRole("button", {name: "New fleet"}));
  expect(push).toHaveBeenCalledWith(`/w/${WORKSPACE}/fleets/new?library=${LIBRARY_CARD.id}&revision=${LIBRARY.head.id}`);
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
  fireEvent.click(await screen.findByRole("button", {name: "Retry Library"}));
  await screen.findByText(LIBRARY_CARD.name);
  expect(fetcher).toHaveBeenCalledTimes(2); expect(screen.queryByRole("alert")).toBeNull();
});

it.each([404, 503, 200])("keeps a refused or incompatible library read separate from empty results (%i)", async (status) => {
  const fetcher = vi.fn().mockResolvedValueOnce(response({entries: []}, status))
    .mockResolvedValueOnce(response({items: [LIBRARY_CARD], next_cursor: null}));
  vi.stubGlobal("fetch", fetcher);
  render(<LibraryBrowser {...PROPS} />);
  await screen.findByText("Fleet library is unavailable right now");
  expect(screen.getByRole("alert").getAttribute("data-slot")).toBe("unavailable-state");
  expect(screen.queryByText(/No (published setups are available yet|setups have been published by this workspace)/)).toBeNull();
  expect(screen.queryByText(/This published setup is no longer available/)).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Retry Library"}));
  await screen.findByText(LIBRARY_CARD.name);
  expect(screen.queryByRole("alert")).toBeNull();
});

it("removes stale actions while inspecting history and uses the selected immutable revision", async () => {
  const historical = Promise.withResolvers<Response>();
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const url = new URL(input, "https://app.test");
    if (url.pathname.endsWith("/fleet-libraries")) return response({items: [LIBRARY_CARD], next_cursor: null});
    if (url.pathname.endsWith("/revisions")) return response({items: [{id: FLEET_ID, revision: "0", canonical_digest: "older", created_at: "1970-01-01T00:00:00Z"}], next_cursor: null});
    if (url.searchParams.get("revision_id") === FLEET_ID) return historical.promise;
    return response(LIBRARY);
  }));
  render(<LibraryBrowser {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Inspect"}));
  await screen.findByRole("button", {name: "New fleet"});
  fireEvent.click(await screen.findByRole("button", {name: "Inspect revision 0"}));
  expect(screen.queryByRole("button", {name: "New fleet"})).toBeNull();
  expect(screen.getByText("Loading selected revision")).not.toBeNull();
  await act(async () => historical.resolve(response({...LIBRARY, head: {...LIBRARY.head, id: FLEET_ID, revision: "0"}})));
  fireEvent.click(await screen.findByRole("button", {name: "New fleet"}));
  expect(PROPS.onSelect).toHaveBeenCalledWith({kind: SOURCE_KIND.library, library_id: LIBRARY_CARD.id, revision_id: FLEET_ID});
});

it("aborts inspection on close and ignores its delayed response", async () => {
  const inspection = Promise.withResolvers<Response>();
  let signal: AbortSignal | undefined;
  vi.stubGlobal("fetch", vi.fn(async (input: string, init: RequestInit) => {
    const path = new URL(input, "https://app.test").pathname;
    if (path.endsWith("/fleet-libraries")) return response({items: [LIBRARY_CARD], next_cursor: null});
    if (path.endsWith("/revisions")) return response(EMPTY_HISTORY);
    signal = init.signal ?? undefined; return inspection.promise;
  }));
  const view = render(<LibraryBrowser {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Inspect"}));
  await waitFor(() => expect(signal).toBeDefined());
  view.unmount();
  expect(signal?.aborted).toBe(true);
  await act(async () => inspection.resolve(response(LIBRARY)));
  expect(screen.queryByRole("button", {name: "New fleet"})).toBeNull();
  expect(PROPS.onSelect).not.toHaveBeenCalled();
});

it.each([200, 503])("can inspect the same recipe again after its first answer is %i", async (status) => {
  let inspections = 0;
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const path = new URL(input, "https://app.test").pathname;
    if (path.endsWith("/fleet-libraries")) return response({items: [LIBRARY_CARD], next_cursor: null});
    if (path.endsWith("/revisions")) return response(EMPTY_HISTORY);
    inspections += 1;
    return response(LIBRARY, inspections === 1 ? status : 200);
  }));
  render(<LibraryBrowser {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Inspect"}));
  if (status === 503) await screen.findByRole("alert");
  else await screen.findByRole("button", {name: "New fleet"});
  fireEvent.click(screen.getByRole("button", {name: "Inspect"}));
  await screen.findByRole("button", {name: "New fleet"});
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
  fireEvent.mouseDown(screen.getByRole("tab", {name: "Workspace"}));
  fireEvent.click(await screen.findByRole("button", {name: "Remove from this workspace"}));
  expect(removeEntry).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", {name: "Remove"}));
  expect(screen.getByRole("button", {name: "Working…"}).hasAttribute("disabled")).toBe(true);
  expect(removeEntry).toHaveBeenCalledOnce();
  expect(removeEntry).toHaveBeenCalledWith(WORKSPACE, LIBRARY_CARD.id);
  await act(async () => pending.resolve({ok: false, error: "Not permitted", status: 403}));
  expect(screen.getByRole("alert").textContent).toContain("Not permitted");
  expect(screen.getByText(LIBRARY_CARD.name)).toBeTruthy();
});

it("can page recipes and revision history and inspect the same historical revision again", async () => {
  const paths: string[] = [];
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    paths.push(input); const url = new URL(input, "https://app.test");
    if (url.pathname.endsWith("/fleet-libraries")) return response({items: [LIBRARY_CARD], next_cursor: url.searchParams.has("starting_after") ? null : "page-two"});
    if (url.pathname.endsWith("/revisions")) return response({items: [{id: FLEET_ID, revision: "0", canonical_digest: "older", created_at: "1970-01-01T00:00:00Z"}], next_cursor: url.searchParams.has("starting_after") ? null : "older"});
    return response({...LIBRARY, head: {...LIBRARY.head, id: url.searchParams.get("revision_id")}});
  }));
  render(<LibraryBrowser {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Next page"}));
  await waitFor(() => expect(screen.queryByRole("button", {name: "Next page"})).toBeNull());
  fireEvent.click(screen.getByRole("button", {name: "Inspect"}));
  fireEvent.click(await screen.findByRole("button", {name: "Older revisions"}));
  await waitFor(() => expect(screen.queryByRole("button", {name: "Older revisions"})).toBeNull());
  for (let attempt = 0; attempt < 2; attempt += 1) {
    fireEvent.click(screen.getByRole("button", {name: "Inspect revision 0"}));
    await screen.findByRole("button", {name: "New fleet"});
  }
  expect(paths.some((path) => path.includes("starting_after=page-two"))).toBe(true);
  expect(paths.some((path) => path.includes("starting_after=older"))).toBe(true);
});

it.each([403, 503])("handles revision history refusal %i with the correct recovery scope", async (status) => {
  let reads = 0;
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const path = new URL(input, "https://app.test").pathname;
    if (path.endsWith("/fleet-libraries")) return response({items: [LIBRARY_CARD], next_cursor: null});
    if (path.endsWith("/revisions")) { reads += 1; return response(EMPTY_HISTORY, reads === 1 ? status : 200); }
    return response(LIBRARY);
  }));
  render(<LibraryBrowser {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Inspect"}));
  const alert = await screen.findByRole("alert");
  expect(alert.getAttribute("data-slot")).toBe("unavailable-state");
  if (status === 403) {
    expect(screen.queryByRole("button", {name: "New fleet"})).toBeNull();
    fireEvent.click(screen.getByRole("button", {name: "Retry Library"}));
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
  fireEvent.mouseDown(screen.getByRole("tab", {name: "Workspace"}));
  fireEvent.click(await screen.findByRole("button", {name: "Remove from this workspace"}));
  fireEvent.click(screen.getByRole("button", {name: "Remove"}));
  await screen.findByText("Removal could not be confirmed. Refresh the Library before trying again.");
  fireEvent.click(screen.getByRole("button", {name: "Remove"}));
  await screen.findByText(/No (published setups are available yet|setups have been published by this workspace)/);
  expect(screen.queryByRole("alertdialog")).toBeNull();
  fireEvent.mouseDown(screen.getByRole("tab", {name: "All"}));
  await screen.findByText(/No (published setups are available yet|setups have been published by this workspace)/);
});


it("explains library ownership and offers a new fleet from an empty library", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => response(EMPTY_HISTORY)));
  render(<LibraryBrowser {...PROPS} />);
  expect(screen.getByText(/Private setups from other accounts are never included/)).toBeTruthy();
  const link = await screen.findByRole("link", {name: "New fleet"});
  expect(link.getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/new`);
  fireEvent.mouseDown(screen.getByRole("tab", {name: "Workspace"}));
  await screen.findByText(/No setups have been published by this workspace/);
  expect(screen.getByText("Setups published by your account, available in this workspace.")).toBeTruthy();
});
