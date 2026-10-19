import { StrictMode } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { DraftClient } from "@/lib/api/drafts";
import { DRAFT, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";
import { DraftSidebarHistory } from "./DraftSidebarHistory";
import { DraftSidebarProvider } from "./draft-sidebar-state";
import { useDraftSidebarBridge } from "./use-draft-sidebar-bridge";

const {identity, replace} = vi.hoisted(() => ({identity: {userId: "reader"}, replace: vi.fn()}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => identity}));
vi.mock("next/navigation", () => ({useSearchParams: () => new URLSearchParams(), useRouter: () => ({replace})}));
const MATCH = {...DRAFT, id: "0199a801-1000-7000-8000-00000000000a", title: "Morning brief",
  linked_fleet_id: null, updated_at: "2026-10-19T00:00:00Z", expires_at: null,
  candidate_revision: 3, setup_revision: null, publication_revision: null};
const SECOND = {...MATCH, id: "0199a801-1000-7000-8000-00000000000b", title: "Release monitor"};
const CURSOR = "opaque+search/cursor";
const client = new DraftClient({account: "account", workspace: WORKSPACE, subject: "reader"});
const initial = {...client.getSnapshot(), draft: DRAFT, listStatus: "ready" as const};

function Bridge() { useDraftSidebarBridge(client, initial, WORKSPACE, "reader", false); return null; }
function tree() {
  return <StrictMode><DraftSidebarProvider><Bridge /><DraftSidebarHistory workspace={WORKSPACE} onNavigate={() => {}} /></DraftSidebarProvider></StrictMode>;
}
function search(value: string) { fireEvent.change(screen.getByRole("textbox", {name: /Search.*chats/i}), {target: {value}}); }
afterEach(() => {cleanup(); vi.restoreAllMocks(); identity.userId = "reader";});

it("searches message text across all private chats, binds paging to the query and restores loaded history", async () => {
  const requests: URL[] = [];
  vi.spyOn(globalThis, "fetch").mockImplementation(async input => {
    const url = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, "https://app.example.test"); requests.push(url);
    return response(url.searchParams.has("starting_after") ? {items: [MATCH, SECOND], next_cursor: null}
      : {items: [MATCH], next_cursor: CURSOR});
  });
  render(tree());
  search("  deployment failures  ");
  expect(await screen.findByRole("link", {name: MATCH.title})).toBeTruthy();
  expect(screen.queryByRole("link", {name: DRAFT.title})).toBeNull();
  expect(requests[0]?.pathname).toBe(`/live/v1/workspaces/${WORKSPACE}/drafts`);
  expect(requests[0]?.searchParams.get("q")).toBe("deployment failures");
  fireEvent.click(screen.getByRole("button", {name: "Load more chats"}));
  expect(await screen.findByRole("link", {name: SECOND.title})).toBeTruthy();
  expect(screen.getAllByRole("link", {name: MATCH.title})).toHaveLength(1);
  expect(requests.map(url => url.searchParams.get("q"))).toEqual(["deployment failures", "deployment failures"]);
  expect(requests[1]?.searchParams.get("starting_after")).toBe(CURSOR);
  search("");
  expect(screen.getByRole("link", {name: DRAFT.title})).toBeTruthy();
  expect(screen.queryByRole("link", {name: MATCH.title})).toBeNull();
});

it("shows a failed search with a retry instead of reporting no matches", async () => {
  const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValue(response({}, 503));
  render(tree()); search("failures");
  expect(await screen.findByRole("button", {name: "Retry search"})).toBeTruthy();
  expect(screen.queryByText("No matching chats")).toBeNull();
  fetcher.mockResolvedValueOnce(response({items: [], next_cursor: null}));
  fireEvent.click(screen.getByRole("button", {name: "Retry search"}));
  expect(await screen.findByText("No matching chats")).toBeTruthy();
});

it("reuses confirmation to delete a matching chat without navigating away from the current chat", async () => {
  const methods: string[] = [];
  vi.spyOn(globalThis, "fetch").mockImplementation(async (_input, options) => {
    methods.push(options?.method ?? "GET");
    return options?.method === "DELETE" ? response(null, 204) : response({items: [MATCH], next_cursor: null});
  });
  render(tree()); search("deployment");
  fireEvent.click(await screen.findByRole("button", {name: `Delete chat: ${MATCH.title}`}));
  const dialog = await screen.findByRole("alertdialog", {name: "Delete setup chat?"});
  expect(methods).toEqual(["GET"]);
  fireEvent.click(within(dialog).getByRole("button", {name: "Delete chat"}));
  await waitFor(() => expect(screen.queryByRole("link", {name: MATCH.title})).toBeNull());
  expect(methods).toEqual(["GET", "DELETE"]);
  expect(replace).not.toHaveBeenCalled();
});

it("aborts the private search on identity loss and never exposes its late reply", async () => {
  const pending = Promise.withResolvers<Response>();
  const observed: {signal: AbortSignal | null} = {signal: null};
  vi.spyOn(globalThis, "fetch").mockImplementation(async (_input, options) => {observed.signal = options?.signal ?? null; return pending.promise;});
  const view = render(tree()); search("deployment");
  await waitFor(() => expect(observed.signal).not.toBeNull());
  identity.userId = "someone-else"; view.rerender(tree());
  expect(observed.signal?.aborted).toBe(true);
  await act(async () => pending.resolve(response({items: [MATCH], next_cursor: null})));
  expect(screen.queryByRole("link", {name: MATCH.title})).toBeNull();
});
