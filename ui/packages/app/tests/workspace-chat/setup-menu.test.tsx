import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS, editChatTitle } from "./fixture";
import { DraftChatHeading } from "@/components/domain/setup/conversation/DraftChatHeading";
import { WorkspaceChat } from "@/components/domain/setup/conversation/WorkspaceChat";
import { CHAT_PLACEHOLDER } from "@/components/domain/setup/chat-defaults";
import { DraftClient } from "@/lib/api/drafts";
import { DRAFT, FLEET, REVIEW, REVISION_ID, response } from "@/tests/helpers/draft-fixtures";

const EDIT_TITLE = "Click to edit the chat name";
const SAVE = "Save fleet";
let fixture: ChatFixture;
let client: DraftClient;
beforeEach(async () => {
  fixture = new ChatFixture(); fixture.mount();
  client = new DraftClient({...CHAT_PROPS, subject: "creator"});
  await client.initialize(); await client.resume(DRAFT.id);
});
afterEach(() => {cleanup(); client.dispose(); vi.unstubAllGlobals();});

it("starts a fresh setup from the empty heading action without writing a draft", () => {
  client.fresh();
  const fresh = vi.spyOn(client, "fresh");
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={client.getSnapshot()} />);
  fireEvent.click(screen.getByRole("button", {name: "New fleet"}));
  expect(fresh).toHaveBeenCalledOnce();
  expect(client.getSnapshot().draft).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it.each([false, true])("hides private actions without an accessible setup (inaccessible: %s)", (inaccessible) => {
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={{...client.getSnapshot(), draft: null, preview: null, inaccessible}} />);
  expect(screen.queryByTitle(EDIT_TITLE)).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it("uses the draft title and keeps save and instruction editing out of the heading", () => {
  const review = {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}};
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={{...client.getSnapshot(), draft: {...DRAFT, linked_fleet_id: FLEET.id, review}, fleet: FLEET}} />);
  expect(screen.queryByRole("button", {name: "Edit instructions"})).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByText("Draft", {exact: true})).toBeTruthy();
  expect(screen.queryByTitle(EDIT_TITLE)).toBeNull();
  expect(screen.queryByRole("button", {name: SAVE})).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it("rejects a rename submitted just as setup becomes busy", async () => {
  const current = client.getSnapshot();
  const view = render(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={current} />);
  await editChatTitle();
  const input = screen.getByRole("textbox", {name: "Chat name"});
  fireEvent.change(input, {target: {value: "My fleet"}});
  view.rerender(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={{...current, busy: true}} />);
  fireEvent.keyDown(input, {key: "Enter"});
  expect(input).toHaveProperty("value", "My fleet");
  fireEvent.keyDown(input, {key: "Escape"});
  fireEvent.click(screen.getByRole("heading", {name: DRAFT.title}));
  expect(screen.queryByLabelText("Chat name")).toBeNull();
  expect(fixture.writes()).toEqual([]);
});

it("saves from the keyboard, restores focus and clears the confirmation tint", async () => {
  vi.useFakeTimers();
  try {
    const saving = Promise.withResolvers<Response>();
    fixture.fetch.mockReturnValueOnce(saving.promise);
    render(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={client.getSnapshot()} />);
    const heading = screen.getByTitle(EDIT_TITLE);
    fireEvent.keyDown(heading, {key: "Tab"});
    fireEvent.keyDown(heading, {key: "Enter", isComposing: true});
    expect(screen.queryByLabelText("Chat name")).toBeNull();
    fireEvent.keyDown(heading, {key: "F2"});
    const input = screen.getByRole("textbox", {name: "Chat name"});
    expect(document.activeElement).toBe(input);
    const name = "Release evidence and checklist ".repeat(4).trim();
    fireEvent.change(input, {target: {value: name}});
    fireEvent.keyDown(input, {key: "Enter", isComposing: true});
    expect(fixture.writes()).toHaveLength(0);
    fireEvent.keyDown(input, {key: "Enter"});
    fireEvent.keyDown(input, {key: "Escape"});
    expect(input).toHaveProperty("readOnly", true);
    fireEvent.change(input, {target: {value: "Attempt to edit during a save"}});
    fireEvent.blur(input);
    const write = fixture.fetch.mock.calls.at(-1)?.[1];
    expect(write?.method).toBe("PATCH");
    expect(new Headers(write?.headers).get("If-Match")).toBe(DRAFT.etag);
    expect(write?.body).toBe(JSON.stringify({title: name}));
    await act(async () => saving.resolve(response({...DRAFT, title: name})));
    expect(document.activeElement).toBe(heading);
    expect(heading.classList.contains("text-success")).toBe(true);
    await act(async () => vi.runOnlyPendingTimersAsync());
    expect(heading.classList.contains("text-success")).toBe(false);
  } finally {
    vi.useRealTimers();
  }
});

it("refuses the title write if its originating draft is no longer active", async () => {
  await expect(client.rename(REVISION_ID, "Keep the other title")).rejects.toMatchObject({status: 409});
  expect(fixture.writes()).toHaveLength(0);
});

it("leaves a newer title intact when the open editor was unchanged", async () => {
  const original = client.getSnapshot();
  const view = render(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={original} />);
  await editChatTitle();
  const renamed = "Updated from a different session";
  view.rerender(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client}
    view={{...original, draft: {...DRAFT, title: renamed}}} />);
  fireEvent.blur(screen.getByRole("textbox", {name: "Chat name"}));
  expect(screen.getByRole("heading", {name: renamed})).toBeTruthy();
  expect(fixture.writes()).toEqual([]);
});

it("removes the save action when the current viewer loses access", () => {
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={{...client.getSnapshot(), inaccessible: true}} />);
  expect(screen.queryByRole("button", {name: SAVE})).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it("loads setup inspection when the summary is opened while preserving the draft", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  const input = await screen.findByRole("textbox", {name: CHAT_PLACEHOLDER});
  fireEvent.change(input, {target: {value: "Preserve this unsent setup request"}});
  expect(await screen.findByRole("region", {name: "Repositories"})).toBeTruthy();
  expect(screen.getByRole("textbox", {name: CHAT_PLACEHOLDER, hidden: true})).toHaveProperty("value", "Preserve this unsent setup request");
});

it("keeps an unfinished private chat identifiable before its first recorded message", () => {
  render(<DraftChatHeading client={client} view={{...client.getSnapshot(), messages: [], draft: {...DRAFT, review: undefined, candidate: undefined}}}
    workspace={CHAT_PROPS.workspace} />);
  expect(screen.getByRole("heading", {name: DRAFT.title})).toBeTruthy();
  expect(screen.getByTitle(EDIT_TITLE)).toBeTruthy();
});
