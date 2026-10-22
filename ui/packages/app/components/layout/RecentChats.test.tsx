import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { RecentFleetList, StandaloneDraftHistory } from "./RecentChats";
import { deleteFleetAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";
import { DRAFT, FLEET_DETAIL, FLEET_ID, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";

const navigation = vi.hoisted(() => ({path: "", replace: vi.fn(), refresh: vi.fn()}));
vi.mock("next/navigation", () => ({useRouter: () => navigation, usePathname: () => navigation.path, useSearchParams: () => new URLSearchParams()}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({deleteFleetAction: vi.fn()}));
const SECOND_ID = "0199a801-1000-7000-8000-000000000008";
const active = {...FLEET_DETAIL, description: "Reviews incoming pull requests", status: "active"};
const stopped = {...active, id: SECOND_ID, name: "Stopped fleet", status: "killed"};
type Call = {path: string; init?: RequestInit};
let calls: Call[];
let freshStatus: string;
let patchStatus: number;
const fetcher = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
  const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  calls.push({path, init});
  if (path.includes("/drafts")) return response({items: [
    {...DRAFT, updated_at: "1970-01-01T00:00:00Z", expires_at: null, linked_fleet_id: null, candidate_revision: 1, setup_revision: null, publication_revision: null},
    {...DRAFT, id: SECOND_ID, title: "Saved setup must not duplicate fleet", updated_at: "1970-01-01T00:00:00Z", expires_at: null, linked_fleet_id: FLEET_ID, candidate_revision: 1, setup_revision: 1, publication_revision: null},
  ], next_cursor: null});
  if (path.endsWith("/fleets?limit=50")) return response({items: [active, stopped], total: null, next_cursor: null});
  if (init?.method === "PATCH") return patchStatus === 200 ? response({...active, status: "killed", etag: '"stopped-fresh"'})
    : response({user_message: "Fleet changed before it could be stopped."}, patchStatus);
  return response({...active, status: freshStatus, etag: '"fresh"'});
});

beforeEach(() => {
  calls = []; freshStatus = "active"; patchStatus = 200;
  fetcher.mockClear(); navigation.replace.mockClear(); navigation.refresh.mockClear();
  navigation.path = `/w/${WORKSPACE}/fleets/${FLEET_ID}`;
  vi.mocked(deleteFleetAction).mockReset().mockResolvedValue({ok: true, data: undefined});
  vi.stubGlobal("fetch", fetcher);
});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it("renders a compact icon-free union with brand color only for active fleets and no model request", async () => {
  const navigate = vi.fn();
  render(<><RecentFleetList workspace={WORKSPACE} onNavigate={navigate} />
    <StandaloneDraftHistory workspace={WORKSPACE} subject="creator" onNavigate={navigate} /></>);
  const fleet = await screen.findByRole("link", {name: active.name});
  expect(fleet.getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/${FLEET_ID}`);
  expect(fleet.getAttribute("title")).toContain("active");
  expect(fleet.querySelector("svg")).toBeNull();
  expect(within(fleet).getByText(active.name).classList.contains("text-pulse")).toBe(true);
  const neutral = screen.getByRole("link", {name: stopped.name});
  expect(within(neutral).getByText(stopped.name).classList.contains("text-pulse")).toBe(false);
  expect(await screen.findByRole("link", {name: DRAFT.title})).toBeTruthy();
  expect(screen.queryByText("Saved setup must not duplicate fleet")).toBeNull();
  const trash = screen.getByRole("button", {name: `Delete fleet: ${active.name}`});
  expect(trash.classList.contains("group-focus-within:opacity-100")).toBe(true);
  expect(trash.querySelector("svg")?.getAttribute("width")).toBe("14");
  fireEvent.click(fleet);
  expect(navigate).toHaveBeenCalledOnce();
  expect(calls.some(call => call.path.includes("/models"))).toBe(false);
});

async function openDelete(name = active.name) {
  render(<RecentFleetList workspace={WORKSPACE} onNavigate={() => {}} />);
  fireEvent.click(await screen.findByRole("button", {name: `Delete fleet: ${name}`}));
  return screen.findByRole("alertdialog");
}

it("stops with the freshly read version before deletion, removes the row and leaves the deleted page", async () => {
  const dialog = await openDelete();
  fireEvent.click(within(dialog).getByRole("button", {name: "Stop and delete"}));
  await waitFor(() => expect(screen.queryByRole("link", {name: active.name})).toBeNull());
  const patch = calls.find(call => call.init?.method === "PATCH");
  expect(new Headers(patch?.init?.headers).get("If-Match")).toBe('"fresh"');
  expect(typeof patch?.init?.body === "string" ? JSON.parse(patch.init.body) : null).toEqual({status: "killed"});
  expect(deleteFleetAction).toHaveBeenCalledExactlyOnceWith(WORKSPACE, FLEET_ID, '"stopped-fresh"');
  expect(navigation.replace).toHaveBeenCalledWith(`/w/${WORKSPACE}/fleets/new`);
  expect(navigation.refresh).toHaveBeenCalledOnce();
  expect(screen.getByRole("link", {name: stopped.name})).toBeTruthy();
});

it("keeps the row and confirmation open on a stop conflict without issuing delete", async () => {
  patchStatus = 412;
  const dialog = await openDelete();
  fireEvent.click(within(dialog).getByRole("button", {name: "Stop and delete"}));
  await screen.findByText("Fleet changed before it could be stopped.");
  expect(deleteFleetAction).not.toHaveBeenCalled();
  expect(screen.getByRole("link", {name: active.name, hidden: true})).toBeTruthy();
  expect(navigation.replace).not.toHaveBeenCalled();
});

it("skips Stop for a freshly killed fleet and retains a delete failure", async () => {
  freshStatus = "killed";
  vi.mocked(deleteFleetAction).mockResolvedValue({ok: false, status: 409, error: "A newer version prevents deletion."});
  const dialog = await openDelete();
  fireEvent.click(within(dialog).getByRole("button", {name: "Stop and delete"}));
  await screen.findByText("A newer version prevents deletion.");
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
  fireEvent.click(within(dialog).getByRole("button", {name: "Stop and delete"}));
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
    .mockImplementationOnce(async () => response({items: [stopped], total: null, next_cursor: null}));
  const view = render(<RecentFleetList workspace={WORKSPACE} onNavigate={() => {}} />);
  fireEvent.click(await screen.findByRole("button", {name: "Load more fleets"}));
  expect(await screen.findByRole("link", {name: stopped.name})).toBeTruthy();
  expect(fetcher.mock.calls.some(([input]) => (typeof input === "string" ? input : input instanceof URL ? input.href : input.url).includes("starting_after=next"))).toBe(true);
  navigation.path = `/w/${WORKSPACE}/fleets/${SECOND_ID}`;
  const before = calls.length;
  view.rerender(<RecentFleetList workspace={WORKSPACE} onNavigate={() => {}} />);
  await waitFor(() => expect(calls.length).toBeGreaterThan(before));
});

it("cancels confirmation and deletes a stopped fleet without leaving another fleet page", async () => {
  const dialog = await openDelete(stopped.name);
  fireEvent.click(within(dialog).getByRole("button", {name: "Cancel"}));
  await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  fireEvent.click(screen.getByRole("button", {name: `Delete fleet: ${stopped.name}`}));
  freshStatus = "killed";
  fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", {name: "Delete fleet"}));
  await waitFor(() => expect(screen.queryByRole("link", {name: stopped.name})).toBeNull());
  expect(navigation.replace).not.toHaveBeenCalled();
});

it("shows private history failure and loads another private chat page", async () => {
  fetcher.mockImplementationOnce(async () => response({}, 503));
  const view = render(<StandaloneDraftHistory workspace={WORKSPACE} subject="creator" onNavigate={() => {}} />);
  expect(await screen.findByRole("status")).toBeTruthy();
  view.unmount();
  fetcher.mockImplementationOnce(async () => response({items: [], next_cursor: "more"}));
  render(<StandaloneDraftHistory workspace={WORKSPACE} subject="creator" onNavigate={() => {}} />);
  fireEvent.click(await screen.findByRole("button", {name: "Load more chats"}));
  expect(await screen.findByRole("link", {name: DRAFT.title})).toBeTruthy();
});

it("removes a private draft from Recent after its authorized delete completes", async () => {
  render(<StandaloneDraftHistory workspace={WORKSPACE} subject="creator" onNavigate={() => {}} />);
  fireEvent.click(await screen.findByRole("button", {name: `Delete chat: ${DRAFT.title}`}));
  const dialog = await screen.findByRole("alertdialog");
  fetcher.mockImplementationOnce(async () => new Response(null, {status: 204}));
  fireEvent.click(within(dialog).getByRole("button", {name: "Delete chat"}));
  await waitFor(() => expect(screen.queryByRole("link", {name: DRAFT.title})).toBeNull());
  expect(fetcher.mock.calls.some(([, init]) => init?.method === "DELETE")).toBe(true);
});

it("coalesces two confirmation events before React commits the pending button state", async () => {
  const dialog = await openDelete();
  const pending = Promise.withResolvers<Response>();
  fetcher.mockImplementationOnce(async () => pending.promise);
  const button = within(dialog).getByRole("button", {name: "Stop and delete"});
  const before = fetcher.mock.calls.length;
  act(() => {fireEvent.click(button); fireEvent.click(button);});
  expect(fetcher.mock.calls.length - before).toBe(1);
  await act(async () => pending.resolve(response({...active, status: "killed", etag: '"fresh"'})));
  await waitFor(() => expect(deleteFleetAction).toHaveBeenCalledOnce());
});
