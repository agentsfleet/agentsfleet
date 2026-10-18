import { Profiler, useSyncExternalStore } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { DRAFT, MESSAGE, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";
import { DraftSidebarHistory, DraftSidebarProvider, useDraftSidebarBridge, useFreshFleetChat } from "./DraftSidebarHistory";
import { LOAD_MORE_COPY } from "@/components/domain/chat/DraftLoadMore";
import { Button } from "@agentsfleet/design-system";

const { identity } = vi.hoisted(() => ({ identity: { userId: "reader" } }));
vi.mock("@/lib/auth/client", () => ({ useCurrentUser: () => identity }));
vi.mock("next/navigation", () => ({ useSearchParams: () => new URLSearchParams(), useRouter: () => ({replace: vi.fn()}) }));
const client = new DraftClient({ account: "account", workspace: WORKSPACE, subject: "reader" });
const fresh = vi.spyOn(client, "fresh").mockImplementation(() => {});
const initial: DraftView = { ...client.getSnapshot(), draft: DRAFT, listStatus: "ready" };
const commits = vi.fn();
const NEXT_DRAFT_ID = "0199a801-1000-7000-8000-00000000000a";

it("keeps loaded chats visible while a page is pending and after it fails", async () => {
  const current = new DraftClient({account: "account", workspace: WORKSPACE, subject: "reader"});
  const card = {...DRAFT, linked_fleet_id: null, updated_at: "1970-01-01T00:00:00Z", expires_at: null};
  const next = Promise.withResolvers<Response>();
  const fetcher = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.includes("/models?")) return response({items: [], next_cursor: null});
    return path.includes("starting_after=") ? next.promise : response({items: [card], next_cursor: "next"});
  });
  try {
    await current.initialize();
    render(<DraftSidebarProvider><LiveBridge current={current} />{sidebar}</DraftSidebarProvider>);
    fireEvent.click(screen.getByRole("button", {name: LOAD_MORE_COPY.chats.label}));
    expect(screen.getByRole("button", {name: "Loading…"})).toHaveProperty("disabled", true);
    expect(screen.getByRole("link", {name: card.title})).toBeTruthy();
    await act(async () => next.resolve(response({}, 503)));
    expect(screen.getByRole("alert").textContent).toContain(LOAD_MORE_COPY.chats.failure);
    expect(screen.getByRole("link", {name: card.title})).toBeTruthy();
    expect(screen.getByRole("button", {name: LOAD_MORE_COPY.chats.label})).toHaveProperty("disabled", false);
  } finally {current.dispose(); fetcher.mockRestore();}
});

function Bridge({ view, running = false }: { view: DraftView; running?: boolean }) {
  useDraftSidebarBridge(client, view, WORKSPACE, "reader", running);
  return null;
}
function NewChat() {
  const start = useFreshFleetChat(WORKSPACE);
  return <Button onClick={start}>New fleet</Button>;
}
const sidebar = <Profiler id="history" onRender={commits}>
  <DraftSidebarHistory workspace={WORKSPACE} onNavigate={() => {}} /><NewChat />
</Profiler>;
function tree(view: DraftView, running = false) {
  return <DraftSidebarProvider><Bridge view={view} running={running} />{sidebar}</DraftSidebarProvider>;
}
afterEach(() => { cleanup(); vi.clearAllMocks(); identity.userId = "reader"; });

it("does not commit sidebar changes for reply chunks or unchanged history snapshots", () => {
  const view = render(tree(initial, true));
  const count = commits.mock.calls.length;
  for (let index = 0; index < 20; index += 1) {
    view.rerender(tree({ ...initial, draft: { ...DRAFT }, cards: [], messages: [{ ...MESSAGE, text: `Chunk ${index}` }] }, true));
  }
  expect(commits).toHaveBeenCalledTimes(count);
  expect(screen.getByRole("link", { name: DRAFT.title }).getAttribute("href")).toContain(`draft=${DRAFT.id}`);
});

it("searches loaded setup-chat titles without network reads and exposes unavailable history after access loss", () => {
  const fetcher = vi.spyOn(globalThis, "fetch");
  const view = render(tree(initial));
  fireEvent.change(screen.getByRole("textbox", { name: "Search loaded chats" }), { target: { value: "FAILURES" } });
  expect(screen.getByRole("link", { name: DRAFT.title })).toBeTruthy();
  fireEvent.change(screen.getByRole("textbox", { name: "Search loaded chats" }), { target: { value: "unknown" } });
  expect(screen.getByText("No matching chats")).toBeTruthy();
  view.rerender(tree({ ...initial, inaccessible: true, listStatus: "unavailable" }));
  expect(screen.queryByRole("link", { name: DRAFT.title })).toBeNull();
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.getByText("Chat history is unavailable.")).toBeTruthy();
  expect(fetcher).not.toHaveBeenCalled();
  fetcher.mockRestore();
});

it("distinguishes failed, loading, and empty history without offering an empty search", () => {
  const empty = client.getSnapshot();
  const view = render(tree({...empty, busy: true}));
  expect(screen.getByText("Loading setup chats…")).toBeTruthy();
  expect(screen.queryByRole("textbox")).toBeNull();
  view.rerender(tree({...empty, error: "We couldn’t load your models and chats.", listStatus: "unavailable"}));
  expect(screen.getByText("Chat history is unavailable.")).toBeTruthy();
  expect(screen.queryByText("Your fleet setup conversations will appear here.")).toBeNull();
  view.rerender(tree({...empty, listStatus: "ready"}));
  expect(screen.getByText("Your fleet setup conversations will appear here.")).toBeTruthy();
  expect(screen.queryByRole("textbox")).toBeNull();
});

it("does not clear a running conversation and permits a fresh one when idle", () => {
  const view = render(tree(initial, true));
  fireEvent.click(screen.getByRole("button", { name: "New fleet" }));
  expect(fresh).not.toHaveBeenCalled();
  view.rerender(tree(initial));
  fireEvent.click(screen.getByRole("button", { name: "New fleet" }));
  expect(fresh).toHaveBeenCalledOnce();
});

it("does not expose another subject's history", () => {
  identity.userId = "another-reader";
  render(tree(initial));
  expect(screen.queryByRole("link", { name: DRAFT.title })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "New fleet" }));
  expect(fresh).not.toHaveBeenCalled();
});

it("keeps a loaded history distinct from a failed or pending chat operation", () => {
  render(tree({...initial, draft: null, error: "Save failed", busy: true}));
  expect(screen.getByText("Your fleet setup conversations will appear here.")).toBeTruthy();
  expect(screen.queryByText("Chat history is unavailable.")).toBeNull();
  expect(screen.queryByText("Loading setup chats…")).toBeNull();
});

it("removes unfinished history when leaving chat and retains completed cards without stale controls", () => {
  const view = render(tree({...initial, draft: null, listStatus: "loading"}));
  expect(screen.getByText("Loading setup chats…")).toBeTruthy();
  view.rerender(<DraftSidebarProvider>{sidebar}</DraftSidebarProvider>);
  expect(screen.queryByText("Loading setup chats…")).toBeNull();
  view.rerender(tree(initial));
  expect(screen.getByRole("link", {name: DRAFT.title})).toBeTruthy();
  view.rerender(<DraftSidebarProvider>{sidebar}</DraftSidebarProvider>);
  expect(screen.getByRole("link", {name: DRAFT.title})).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "New fleet"}));
  expect(fresh).not.toHaveBeenCalled();
});

it("deduplicates the current draft and preserves the other loaded titles", () => {
  const card = {...DRAFT, linked_fleet_id: null, updated_at: "1970-01-01T00:00:00Z", expires_at: null};
  render(tree({...initial, cards: [card, {...card, id: "another-chat", title: "Another setup"}]}));
  expect(screen.getAllByRole("link")).toHaveLength(2);
  expect(screen.getAllByRole("link", {name: DRAFT.title})).toHaveLength(1);
  expect(screen.getByRole("link", {name: "Another setup"})).toBeTruthy();
});

function LiveBridge({current}: {current: DraftClient}) {
  const view = useSyncExternalStore(current.subscribe, current.getSnapshot, current.getSnapshot);
  useDraftSidebarBridge(current, view, WORKSPACE, "reader", false);
  return null;
}

it("loads the next chat page on demand and makes its title searchable", async () => {
  const current = new DraftClient({account: "account", workspace: WORKSPACE, subject: "reader"});
  const card = {...DRAFT, linked_fleet_id: null, updated_at: "1970-01-01T00:00:00Z", expires_at: null};
  const fetcher = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
    const url = (typeof input === "string" ? input : input instanceof URL ? input.href : input.url);
    if (url.includes("/models?")) return response({items: [], next_cursor: null});
    return response(url.includes("starting_after=")
      ? {items: [{...card, id: NEXT_DRAFT_ID, title: "Next setup"}], next_cursor: null}
      : {items: [card], next_cursor: "next"});
  });
  try {
    await current.initialize();
    render(<DraftSidebarProvider><LiveBridge current={current} />{sidebar}</DraftSidebarProvider>);
    fireEvent.click(screen.getByRole("button", {name: "Load more chats"}));
    expect(await screen.findByRole("link", {name: "Next setup"})).toBeTruthy();
    await waitFor(() => expect(screen.queryByRole("button", {name: "Load more chats"})).toBeNull());
    fireEvent.change(screen.getByRole("textbox", {name: "Search loaded chats"}), {target: {value: "Next"}});
    expect(screen.getAllByRole("link")).toHaveLength(1);
    expect(fetcher).toHaveBeenCalledTimes(3);
  } finally { cleanup(); current.dispose(); fetcher.mockRestore(); }
});

it("keeps the selected second chat in place and exposes its setup version", () => {
  const card = {...DRAFT, linked_fleet_id: null, updated_at: "1970-01-01T00:00:00Z", expires_at: null};
  render(tree({...initial, cards: [{...card, id: NEXT_DRAFT_ID, title: "First setup"}, card]}));
  expect(screen.getAllByRole("link").map((link) => link.getAttribute("aria-label"))).toEqual(["First setup", DRAFT.title]);
  expect(screen.getByRole("link", {name: DRAFT.title}).getAttribute("aria-describedby")).toBe(`${DRAFT.id}-state`);
  expect(screen.getByText("Setup ready · Setup v1")).toBeTruthy();
});

it("requires confirmation and keeps a chat available after a refused deletion", async () => {
  const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValue(response({user_message: "Finish the current request first"}, 409));
  try {
    render(tree(initial));
    fireEvent.click(screen.getByRole("button", {name: `Delete chat: ${DRAFT.title}`}));
    const dialog = screen.getByRole("alertdialog", {name: "Delete setup chat?"});
    expect(dialog.textContent).toContain("Saved fleets and published library versions stay available");
    fireEvent.click(within(dialog).getByRole("button", {name: "Cancel"}));
    expect(fetcher).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", {name: `Delete chat: ${DRAFT.title}`}));
    fireEvent.click(screen.getByRole("button", {name: "Delete chat"}));
    await screen.findByText("Finish the current request first");
    expect(screen.getByRole("link", {name: DRAFT.title})).toBeTruthy();
    fetcher.mockResolvedValueOnce(response(null, 204));
    fireEvent.click(screen.getByRole("button", {name: "Delete chat"}));
    await waitFor(() => expect(screen.queryByRole("link", {name: DRAFT.title})).toBeNull());
  } finally {fetcher.mockRestore();}
});
