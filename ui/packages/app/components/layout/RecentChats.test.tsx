import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { RecentFleetList } from "./RecentChats";
import { deleteFleetAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";
import { DRAFT, FLEET_DETAIL, FLEET_ID, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";

const navigation = vi.hoisted(() => ({path: "", replace: vi.fn(), refresh: vi.fn()}));
vi.mock("next/navigation", () => ({useRouter: () => navigation, usePathname: () => navigation.path, useSearchParams: () => new URLSearchParams()}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => ({userId: "owner", isLoaded: true, isSignedIn: true})}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({deleteFleetAction: vi.fn(), setFleetStatusAction: vi.fn()}));
const SECOND_ID = "0199a801-1000-7000-8000-000000000008";
const EDIT_ID = "0199a801-1000-7000-8000-000000000009";
const active = {...FLEET_DETAIL, description: "Reviews incoming pull requests", status: "active"};
const archived = {...active, id: SECOND_ID, name: "Archived fleet", status: "archived"};
type Call = {path: string; init?: RequestInit};
let calls: Call[];
let freshStatus: string;
let readStatus: number;
const fetcher = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
  const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  calls.push({path, init});
  if (path.includes("/drafts")) return response({items: [
    {...DRAFT, updated_at: "1970-01-01T00:00:00Z", expires_at: null, linked_fleet_id: null, candidate_revision: 1, setup_revision: null, publication_revision: null},
    {...DRAFT, id: SECOND_ID, title: "Saved setup must not duplicate fleet", state: "closed", updated_at: "1970-01-01T00:00:00Z", expires_at: null, linked_fleet_id: FLEET_ID, candidate_revision: 1, setup_revision: 1, publication_revision: null},
    {...DRAFT, id: EDIT_ID, title: "Unfinished fleet edit", updated_at: "1970-01-01T00:00:00Z", expires_at: null, linked_fleet_id: FLEET_ID, candidate_revision: 2, setup_revision: 1, publication_revision: null},
  ], next_cursor: null});
  if (path.endsWith("/fleets?limit=50")) return response({items: [active, archived], total: null, next_cursor: null});
  if (readStatus !== 200) return response({user_message: "Fleet could not be read. Try again."}, readStatus);
  return response({...active, status: freshStatus, etag: '"fresh"'});
});

beforeEach(() => {
  calls = []; freshStatus = "active"; readStatus = 200;
  fetcher.mockClear(); navigation.replace.mockClear(); navigation.refresh.mockClear();
  navigation.path = `/w/${WORKSPACE}/fleets/${FLEET_ID}`;
  vi.mocked(deleteFleetAction).mockReset().mockResolvedValue({ok: true, data: undefined});
  vi.stubGlobal("fetch", fetcher);
});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it("shows saved fleets only without fetching private drafts or model choices", async () => {
  const navigate = vi.fn();
  render(<RecentFleetList workspace={WORKSPACE} onNavigate={navigate} />);
  const fleet = await screen.findByRole("link", {name: active.name});
  expect(fleet.getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/${FLEET_ID}`);
  expect(fleet.getAttribute("title")).toContain("active");
  expect(fleet.querySelector("svg")).toBeNull();
  expect(within(fleet).getByText(active.name).classList.contains("text-pulse")).toBe(false);
  const neutral = screen.getByRole("link", {name: `${archived.name} Archived`});
  expect(within(neutral).getByText(archived.name).classList.contains("text-pulse")).toBe(false);
  expect(screen.queryByRole("link", {name: DRAFT.title})).toBeNull();
  expect(screen.queryByText("Saved setup must not duplicate fleet")).toBeNull();
  expect(screen.queryByRole("link", {name: "Unfinished fleet edit"})).toBeNull();
  expect(calls.some(call => call.path.includes("/drafts"))).toBe(false);
  const trash = screen.getByRole("button", {name: `Permanently delete fleet: ${active.name}`});
  expect(trash.classList.contains("group-focus-within:opacity-100")).toBe(true);
  expect(trash.querySelector("svg")?.getAttribute("width")).toBe("14");
  fireEvent.click(fleet);
  expect(navigate).toHaveBeenCalledOnce();
  expect(calls.some(call => call.path.includes("/models"))).toBe(false);
});

it("labels an archived fleet so retained history does not look active", async () => {
  render(<RecentFleetList workspace={WORKSPACE} onNavigate={vi.fn()} />);
  const link = await screen.findByRole("link", {name: `${archived.name} Archived`});
  expect(within(link).getByText("Archived")).toBeTruthy();
  expect(link.getAttribute("title")).toBe(`${archived.name} · archived`);
  expect(link.querySelector(".bg-pulse")).toBeNull();
});

async function openDelete(name = active.name) {
  render(<RecentFleetList workspace={WORKSPACE} onNavigate={() => {}} />);
  fireEvent.click(await screen.findByRole("button", {name: `Permanently delete fleet: ${name}`}));
  return screen.findByRole("alertdialog");
}

it("deletes directly with the freshly read version, removes the row and leaves the deleted page", async () => {
  const dialog = await openDelete();
  fireEvent.click(within(dialog).getByRole("button", {name: "Permanently delete"}));
  await waitFor(() => expect(screen.queryByRole("link", {name: active.name})).toBeNull());
  expect(calls.some(call => call.init?.method === "PATCH")).toBe(false);
  expect(deleteFleetAction).toHaveBeenCalledExactlyOnceWith(WORKSPACE, FLEET_ID, '"fresh"');
  expect(navigation.replace).toHaveBeenCalledWith(`/w/${WORKSPACE}/fleets/new`);
  expect(navigation.refresh).not.toHaveBeenCalled();
  expect(screen.getByRole("link", {name: `${archived.name} Archived`})).toBeTruthy();
});

it("keeps the row and confirmation open when the version read fails without issuing delete", async () => {
  const dialog = await openDelete();
  readStatus = 503;
  fireEvent.click(within(dialog).getByRole("button", {name: "Permanently delete"}));
  await screen.findByText(/Fleet could not be read/);
  expect(deleteFleetAction).not.toHaveBeenCalled();
  expect(screen.getByRole("link", {name: active.name, hidden: true})).toBeTruthy();
  expect(navigation.replace).not.toHaveBeenCalled();
});

it("retains a deletion refusal for a freshly archived fleet", async () => {
  freshStatus = "archived";
  vi.mocked(deleteFleetAction).mockResolvedValue({ok: false, status: 409, error: "A newer version prevents deletion."});
  const dialog = await openDelete();
  fireEvent.click(within(dialog).getByRole("button", {name: "Permanently delete"}));
  await screen.findByText(/A newer version prevents deletion/);
  expect(calls.some(call => call.init?.method === "PATCH")).toBe(false);
  expect(deleteFleetAction).toHaveBeenCalledWith(WORKSPACE, FLEET_ID, '"fresh"');
  expect(screen.getByRole("link", {name: active.name, hidden: true})).toBeTruthy();
});

it("aborts an in-flight version read when its owner unmounts and never deletes afterward", async () => {
  const pending = Promise.withResolvers<Response>();
  const base = fetcher.getMockImplementation()!;
  let signal: AbortSignal | undefined;
  fetcher.mockImplementationOnce(base);
  const dialog = await openDelete();
  fetcher.mockImplementationOnce(async (_input, init) => {signal = init?.signal ?? undefined; return pending.promise;});
  fireEvent.click(within(dialog).getByRole("button", {name: "Permanently delete"}));
  await waitFor(() => expect(signal).toBeDefined());
  cleanup();
  expect(signal?.aborted).toBe(true);
  await act(async () => pending.resolve(response({...active, etag: '"late"'})));
  expect(deleteFleetAction).not.toHaveBeenCalled();
  expect(navigation.replace).not.toHaveBeenCalled();
});

it("recovers an unavailable fleet list through Refresh", async () => {
  fetcher.mockImplementationOnce(async () => response({}, 503));
  render(<RecentFleetList workspace={WORKSPACE} onNavigate={() => {}} />);
  fireEvent.click(await screen.findByRole("button", {name: "Refresh"}));
  expect(await screen.findByRole("link", {name: active.name})).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Refresh"})).toBeNull();
});

it("loads another fleet page and refreshes when navigation changes", async () => {
  fetcher.mockImplementationOnce(async () => response({items: [active], total: null, next_cursor: "next"}))
    .mockImplementationOnce(async () => response({items: [archived], total: null, next_cursor: null}));
  const view = render(<RecentFleetList workspace={WORKSPACE} onNavigate={() => {}} />);
  fireEvent.click(await screen.findByRole("button", {name: "Load more fleets"}));
  expect(await screen.findByRole("link", {name: `${archived.name} Archived`})).toBeTruthy();
  expect(fetcher.mock.calls.some(([input]) => (typeof input === "string" ? input : input instanceof URL ? input.href : input.url).includes("starting_after=next"))).toBe(true);
  navigation.path = `/w/${WORKSPACE}/fleets/${SECOND_ID}`;
  const before = calls.length;
  view.rerender(<RecentFleetList workspace={WORKSPACE} onNavigate={() => {}} />);
  await waitFor(() => expect(calls.length).toBeGreaterThan(before));
});

it("cancels confirmation and deletes an archived fleet without leaving another fleet page", async () => {
  const dialog = await openDelete(archived.name);
  fireEvent.click(within(dialog).getByRole("button", {name: "Cancel"}));
  await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  fireEvent.click(screen.getByRole("button", {name: `Permanently delete fleet: ${archived.name}`}));
  freshStatus = "archived";
  fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", {name: "Permanently delete"}));
  await waitFor(() => expect(screen.queryByRole("link", {name: `${archived.name} Archived`})).toBeNull());
  expect(navigation.replace).not.toHaveBeenCalled();
});

it("coalesces two confirmation events before React commits the pending button state", async () => {
  const dialog = await openDelete();
  const pending = Promise.withResolvers<Response>();
  fetcher.mockImplementationOnce(async () => pending.promise);
  const button = within(dialog).getByRole("button", {name: "Permanently delete"});
  const before = fetcher.mock.calls.length;
  act(() => {fireEvent.click(button); fireEvent.click(button);});
  expect(fetcher.mock.calls.length - before).toBe(1);
  await act(async () => pending.resolve(response({...active, status: "archived", etag: '"fresh"'})));
  await waitFor(() => expect(deleteFleetAction).toHaveBeenCalledOnce());
});
