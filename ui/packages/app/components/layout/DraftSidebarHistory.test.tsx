import { Profiler, useSyncExternalStore } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { DRAFT, FLEET_ID, MESSAGE, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";
import { UnfinishedSetups } from "@/components/domain/chat/UnfinishedSetups";
import { DraftSidebarHistory } from "./DraftSidebarHistory";
import { DraftSidebarItem } from "./DraftSidebarItem";
import { useDraftSidebarBridge } from "./use-draft-sidebar-bridge";
import { DraftSidebarProvider, useFreshFleetChat, removeSidebarDraft, type DraftSidebarState } from "./draft-sidebar-state";
import { LOAD_MORE_COPY } from "@/components/domain/chat/DraftLoadMore";
import { Button } from "@agentsfleet/design-system";

const { identity, replace } = vi.hoisted(() => ({ identity: { userId: "reader" }, replace: vi.fn() }));
vi.mock("@/lib/auth/client", () => ({ useCurrentUser: () => identity }));
vi.mock("next/navigation", () => ({ usePathname: () => `/w/${WORKSPACE}/fleets/new`, useSearchParams: () => new URLSearchParams(), useRouter: () => ({replace}) }));
const client = new DraftClient({ account: "account", workspace: WORKSPACE, subject: "reader" });
const fresh = vi.spyOn(client, "fresh").mockImplementation(() => {});
const initial: DraftView = { ...client.getSnapshot(), draft: DRAFT, listStatus: "ready" };
const commits = vi.fn();
const NEXT_DRAFT_ID = "0199a801-1000-7000-8000-00000000000a";

it.each([false, true])("draft_sidebar_title_and_delete retain navigation typography and ellipsis (%s)", (active) => {
  const title = "A long private setup request with emoji 🦉 and enough detail to overflow the sidebar";
  const remove = vi.fn();
  render(<DraftSidebarItem card={{id: DRAFT.id, title, detail: "In progress", activity: "draft" as const}} workspace={WORKSPACE}
    active={active} disabled={false} onNavigate={() => {}} onDelete={remove} />);
  const link = screen.getByRole("link", {name: title});
  expect(link.getAttribute("title")).toBe(title);
  expect(link.classList.contains("font-sans")).toBe(true);
  expect(link.classList.contains("text-body-sm")).toBe(true);
  expect(link.querySelector("svg")).toBeTruthy();
  expect(screen.getByRole("button", {name: `Delete chat: ${title}`}).querySelector("svg")?.getAttribute("width")).toBe("14");
  expect(screen.getByText("In progress").classList.contains("sr-only")).toBe(true);
  expect(link.classList.contains("items-center")).toBe(true);
  const text = screen.getByText(title);
  expect(text.classList.contains("truncate")).toBe(true);
  expect(text.classList.contains("font-medium")).toBe(false);
  expect(link.getAttribute("data-active")).toBe(active ? "true" : null);
  fireEvent.click(screen.getByRole("button", {name: `Delete chat: ${title}`}));
  expect(remove).toHaveBeenCalledOnce();
});

it("keeps loaded chats visible while a page is pending and after it fails", async () => {
  const current = new DraftClient({account: "account", workspace: WORKSPACE, subject: "reader"});
  const card = {...DRAFT, linked_fleet_id: null, updated_at: "1970-01-01T00:00:00Z", expires_at: null, candidate_revision: null, setup_revision: null, publication_revision: null};
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

function Bridge({ view, running = false, current = client, workspace = WORKSPACE, subject = "reader" }: { view: DraftView; running?: boolean; current?: DraftClient; workspace?: string; subject?: string }) {
  useDraftSidebarBridge(current, view, workspace, subject, running);
  return null;
}
function NewChat() {
  const start = useFreshFleetChat(WORKSPACE);
  return <Button onClick={start}>New fleet</Button>;
}
const sidebar = <Profiler id="history" onRender={commits}>
  <UnfinishedSetups workspace={WORKSPACE} onReload={() => client.initialize()} /><NewChat />
</Profiler>;
function tree(view: DraftView, running = false) {
  return <DraftSidebarProvider><Bridge view={view} running={running} />{sidebar}</DraftSidebarProvider>;
}
afterEach(() => { cleanup(); vi.clearAllMocks(); identity.userId = "reader"; });

it("keeps an empty private draft list quiet beside the account fleet list", async () => {
  const fetcher = vi.spyOn(globalThis, "fetch").mockImplementation(async () => response({items: [], total: null, next_cursor: null}));
  try {
    render(<DraftSidebarProvider><Bridge view={{...initial, draft: null}} />
      <DraftSidebarHistory account="account" workspace={WORKSPACE} onNavigate={() => {}} /></DraftSidebarProvider>);
    await waitFor(() => expect(fetcher).toHaveBeenCalled());
    expect(screen.queryByText("Your setup chats will appear here.")).toBeNull();
    expect(screen.getByText("Fleets")).toBeTruthy();
  } finally {fetcher.mockRestore();}
});

it("keeps the fleet list private when no account is supplied", () => {
  const fetcher = vi.spyOn(globalThis, "fetch");
  render(<DraftSidebarHistory workspace={WORKSPACE} onNavigate={vi.fn()} />);
  expect(screen.queryByText("Fleets")).toBeNull();
  expect(fetcher).not.toHaveBeenCalled();
  fetcher.mockRestore();
});

it("does not commit sidebar changes for reply chunks or unchanged history snapshots", () => {
  const view = render(tree(initial, true));
  const count = commits.mock.calls.length;
  for (let index = 0; index < 20; index += 1) {
    view.rerender(tree({ ...initial, draft: { ...DRAFT }, cards: [], messages: [{ ...MESSAGE, text: `Chunk ${index}` }] }, true));
  }
  expect(commits).toHaveBeenCalledTimes(count);
  expect(screen.getByRole("link", { name: DRAFT.title }).getAttribute("href")).toContain(`draft=${DRAFT.id}`);
});

it("shows resumable unfinished setups in their own section without search", () => {
  render(tree(initial));
  expect(screen.getByText("Unfinished setups")).toBeTruthy();
  expect(screen.getByRole("link", {name: DRAFT.title})).toBeTruthy();
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.queryByText("Fleet setup chats")).toBeNull();
  expect(screen.queryByText("Recents")).toBeNull();
});

it("exposes unavailable history after access loss", () => {
  const fetcher = vi.spyOn(globalThis, "fetch");
  const view = render(tree(initial));
  expect(screen.getByRole("link", { name: DRAFT.title })).toBeTruthy();
  view.rerender(tree({ ...initial, inaccessible: true, listStatus: "unavailable" }));
  expect(screen.queryByRole("link", { name: DRAFT.title })).toBeNull();
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.getByText(/Unfinished setups could not be loaded/)).toBeTruthy();
  expect(fetcher).not.toHaveBeenCalled();
  fetcher.mockRestore();
});

it("distinguishes failed, loading, and empty history", () => {
  const empty = client.getSnapshot();
  const view = render(tree({...empty, busy: true}));
  expect(screen.getByLabelText("Loading unfinished setups")).toBeTruthy();
  expect(screen.queryByRole("textbox")).toBeNull();
  view.rerender(tree({...empty, error: "We couldn’t load your models and chats.", listStatus: "unavailable"}));
  expect(screen.getByText(/Unfinished setups could not be loaded/)).toBeTruthy();
  expect(screen.queryByText("Your setup chats will appear here.")).toBeNull();
  view.rerender(tree({...empty, listStatus: "ready"}));
  expect(screen.queryByRole("region", {name: "Unfinished setups"})).toBeNull();
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

it("keeps pending and failed states accessible without visible status icons", () => {
  const view = render(tree(initial));
  const link = screen.getByRole("link", {name: DRAFT.title});
  expect(link.querySelector("svg")).toBeTruthy();
  view.rerender(tree({...initial, messages: [{...MESSAGE, state: "running"}]}));
  expect(screen.getByText("Waiting for a result").classList.contains("sr-only")).toBe(true);
  expect(link.querySelector("svg")).toBeTruthy();
  view.rerender(tree({...initial, messages: [{...MESSAGE, role: "assistant", state: "failed",
    failure: {reason: "provider_usage_unknown", error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"}}]}));
  expect(screen.getByText("Setup message failed")).toBeTruthy();
  expect(link.querySelector("svg")).toBeTruthy();
  fireEvent.focus(link);
  expect(screen.getByRole("tooltip").textContent).toContain(DRAFT.title);
  expect(screen.getByRole("tooltip").closest("[data-side]")?.className).toContain("ph-no-record");
});

it("does not expose another subject's history", () => {
  identity.userId = "another-reader";
  render(tree(initial));
  expect(screen.queryByRole("link", { name: DRAFT.title })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "New fleet" }));
  expect(fresh).not.toHaveBeenCalled();
});

it("keeps a loaded history distinct from a failed or pending chat operation", () => {
  const view = render(tree(initial));
  view.rerender(tree({...initial, draft: null, cards: [], listStatus: "loading"}));
  expect(screen.getByRole("link", {name: DRAFT.title})).toBeTruthy();
  expect(screen.getByRole("button", {name: `Delete chat: ${DRAFT.title}`})).toHaveProperty("disabled", true);
  view.rerender(tree({...initial, draft: null, error: "Save failed", busy: true}));
  expect(screen.queryByRole("region", {name: "Unfinished setups"})).toBeNull();
  expect(screen.queryByText(/Unfinished setups could not be loaded/)).toBeNull();
  expect(screen.queryByLabelText("Loading unfinished setups")).toBeNull();
});

it("removes unfinished history when leaving chat and retains completed cards without stale controls", () => {
  const view = render(tree({...initial, draft: null, listStatus: "loading"}));
  expect(screen.getByLabelText("Loading unfinished setups")).toBeTruthy();
  view.rerender(<DraftSidebarProvider>{sidebar}</DraftSidebarProvider>);
  expect(screen.queryByLabelText("Loading unfinished setups")).toBeNull();
  view.rerender(tree(initial));
  expect(screen.getByRole("link", {name: DRAFT.title})).toBeTruthy();
  view.rerender(<DraftSidebarProvider>{sidebar}</DraftSidebarProvider>);
  expect(screen.getByRole("link", {name: DRAFT.title})).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "New fleet"}));
  expect(fresh).not.toHaveBeenCalled();
});

it("deduplicates the current draft and preserves the other loaded titles", () => {
  const card = {...DRAFT, linked_fleet_id: null, updated_at: "1970-01-01T00:00:00Z", expires_at: null, candidate_revision: null, setup_revision: null, publication_revision: null};
  const linked = {...DRAFT, linked_fleet_id: FLEET_ID};
  const cards = [{...card, linked_fleet_id: FLEET_ID}, {...card, id: "another-chat", title: "Another setup"}];
  const view = render(tree({...initial, draft: linked, cards}));
  expect(screen.getAllByRole("link")).toHaveLength(2);
  expect(screen.getAllByRole("link", {name: DRAFT.title})).toHaveLength(1);
  expect(screen.getByRole("link", {name: "Another setup"})).toBeTruthy();
  view.rerender(tree({...initial, draft: {...linked, state: "closed"}, cards}));
  expect(screen.queryByRole("link", {name: DRAFT.title})).toBeNull();
  expect(screen.getByRole("link", {name: "Another setup"})).toBeTruthy();
});

function LiveBridge({current}: {current: DraftClient}) {
  const view = useSyncExternalStore(current.subscribe, current.getSnapshot, current.getSnapshot);
  useDraftSidebarBridge(current, view, WORKSPACE, "reader", false);
  return null;
}

it("loads the next chat page on demand and keeps both titles visible", async () => {
  const current = new DraftClient({account: "account", workspace: WORKSPACE, subject: "reader"});
  const card = {...DRAFT, linked_fleet_id: null, updated_at: "1970-01-01T00:00:00Z", expires_at: null, candidate_revision: null, setup_revision: null, publication_revision: null};
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
    expect(screen.getAllByRole("link")).toHaveLength(2);
    expect(fetcher).toHaveBeenCalledTimes(3);
  } finally { cleanup(); current.dispose(); fetcher.mockRestore(); }
});

it("keeps the selected second chat in place and exposes its setup version", () => {
  const card = {...DRAFT, linked_fleet_id: null, updated_at: "1970-01-01T00:00:00Z", expires_at: null, candidate_revision: null, setup_revision: null, publication_revision: null};
  render(tree({...initial, cards: [{...card, id: NEXT_DRAFT_ID, title: "First setup"}, card]}));
  expect(screen.getAllByRole("link").map((link) => link.getAttribute("aria-label"))).toEqual(["First setup", DRAFT.title]);
  expect(screen.getByRole("link", {name: DRAFT.title}).getAttribute("aria-describedby")).toBe(`${DRAFT.id}-state`);
  expect(screen.getByText("Setup ready · Proposed v1")).toBeTruthy();
});

it("requires confirmation and keeps a chat available after a refused deletion", async () => {
  const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValue(response({user_message: "Finish the current request first"}, 409));
  try {
    render(tree(initial));
    fireEvent.click(screen.getByRole("button", {name: `Delete chat: ${DRAFT.title}`}));
    const dialog = await screen.findByRole("alertdialog", {name: "Delete setup chat?"});
    expect(dialog.textContent).toContain("Saved fleets and published library versions stay available");
    fireEvent.click(within(dialog).getByRole("button", {name: "Cancel"}));
    expect(fetcher).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", {name: `Delete chat: ${DRAFT.title}`}));
    fireEvent.click(screen.getByRole("button", {name: "Delete chat"}));
    await screen.findByText("Finish the current request first");
    expect(screen.getByRole("link", {hidden: true}).textContent).toContain(DRAFT.title);
    fetcher.mockResolvedValueOnce(response(null, 204));
    fireEvent.click(screen.getByRole("button", {name: "Delete chat"}));
    await waitFor(() => expect(screen.queryByRole("link", {name: DRAFT.title})).toBeNull());
    expect(replace).toHaveBeenCalledWith(`/w/${WORKSPACE}/fleets/new`);
  } finally {fetcher.mockRestore();}
});

it("deletes another chat without navigating away from the selected conversation", async () => {
  const other = {...DRAFT, id: NEXT_DRAFT_ID, title: "Other setup", linked_fleet_id: null, updated_at: "1970-01-01T00:00:00Z", expires_at: null, candidate_revision: null, setup_revision: null, publication_revision: null};
  const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValue(response(null, 204));
  try {
    render(tree({...initial, cards: [other]}));
    fireEvent.click(screen.getByRole("button", {name: `Delete chat: ${other.title}`}));
    const dialog = await screen.findByRole("alertdialog", {name: "Delete setup chat?"});
    fireEvent.click(within(dialog).getByRole("button", {name: "Delete chat"}));
    await waitFor(() => expect(screen.queryByRole("link", {name: other.title})).toBeNull());
    expect(screen.getByRole("link", {name: DRAFT.title})).toBeTruthy();
    expect(replace).not.toHaveBeenCalled();
  } finally {fetcher.mockRestore();}
});

it.each(["workspace", "subject"] as const)("does not carry loaded private cards into a pending %s", (scope) => {
  const view = render(tree(initial));
  const changed = {[scope]: "other"};
  view.rerender(<DraftSidebarProvider><Bridge view={{...initial, draft: null, listStatus: "loading"}} {...changed} />{sidebar}</DraftSidebarProvider>);
  expect(screen.queryByRole("link", {name: DRAFT.title})).toBeNull();
});

it("retains the newer chat owner's history when an older bridge unmounts", () => {
  const next = new DraftClient({account: "account", workspace: WORKSPACE, subject: "reader"});
  const older = <Bridge key="older" view={initial} />;
  const newer = <Bridge key="newer" current={next} view={{...initial, draft: {...DRAFT, title: "New owner"}}} />;
  const view = render(<DraftSidebarProvider>{older}{sidebar}</DraftSidebarProvider>);
  view.rerender(<DraftSidebarProvider>{older}{newer}{sidebar}</DraftSidebarProvider>);
  expect(screen.getByRole("link", {name: "New owner"})).toBeTruthy();
  view.rerender(<DraftSidebarProvider>{newer}{sidebar}</DraftSidebarProvider>);
  expect(screen.getByRole("link", {name: "New owner"})).toBeTruthy();
  view.rerender(<DraftSidebarProvider><Bridge key="newer" current={next} view={{...initial, listStatus: "unavailable"}} />{sidebar}</DraftSidebarProvider>);
  view.rerender(<DraftSidebarProvider>{sidebar}</DraftSidebarProvider>);
  expect(screen.getByRole("button", {name: `Delete chat: ${DRAFT.title}`})).toHaveProperty("disabled", true);
  next.dispose();
});

it("removes a private card only from its owning scope and clears only the selected chat", () => {
  const scope = {workspace: WORKSPACE, subject: "reader"};
  const value: DraftSidebarState = {...scope, cards: [{id: DRAFT.id, title: DRAFT.title, detail: "Setup ready", activity: "draft" as const}, {id: "other", title: "Other chat", detail: "In progress", activity: "draft" as const}],
    fresh: null, remove: null, owner: null, locked: false, current: DRAFT.id, more: null, status: "ready"};
  expect(removeSidebarDraft(null, scope, DRAFT.id)).toBeNull();
  expect(removeSidebarDraft(value, {...scope, workspace: "other"}, DRAFT.id)).toBe(value);
  expect(removeSidebarDraft(value, {...scope, subject: "other"}, DRAFT.id)).toBe(value);
  expect(removeSidebarDraft(value, scope, "absent")).toBe(value);
  expect(removeSidebarDraft(value, scope, "other")).toMatchObject({current: DRAFT.id, cards: [value.cards[0]]});
  expect(removeSidebarDraft(value, scope, DRAFT.id)).toMatchObject({current: null, cards: [value.cards[1]]});
});
