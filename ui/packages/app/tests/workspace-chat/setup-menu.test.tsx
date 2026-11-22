import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS, chooseSetupAction } from "./fixture";
import { DraftChatHeading } from "@/components/domain/setup/conversation/DraftChatHeading";
import { WorkspaceChat } from "@/components/domain/setup/conversation/WorkspaceChat";
import { CHAT_PLACEHOLDER } from "@/components/domain/setup/chat-defaults";
import { DraftClient } from "@/lib/api/drafts";
import { DRAFT, FLEET, REVIEW } from "@/tests/helpers/draft-fixtures";

const MENU = "Draft actions";
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
  expect(screen.queryByRole("button", {name: MENU})).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it("uses the draft title and keeps save and instruction editing out of the heading", () => {
  const review = {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}};
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={{...client.getSnapshot(), draft: {...DRAFT, linked_fleet_id: FLEET.id, review}, fleet: FLEET}} />);
  expect(screen.queryByRole("button", {name: "Edit instructions"})).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByText("Draft", {exact: true})).toBeTruthy();
  expect(screen.queryByRole("button", {name: MENU})).toBeNull();
  expect(screen.queryByRole("button", {name: SAVE})).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it("rejects a rename submitted just as setup becomes busy", async () => {
  const current = client.getSnapshot();
  const view = render(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={current} />);
  await chooseSetupAction("Rename chat");
  const input = screen.getByRole("textbox", {name: "Chat name"});
  fireEvent.change(input, {target: {value: "My fleet"}});
  view.rerender(<DraftChatHeading workspace={CHAT_PROPS.workspace} client={client} view={{...current, busy: true}} />);
  fireEvent.submit(input.closest("form")!);
  expect(input).toHaveProperty("value", "My fleet");
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
  expect(screen.getByRole("button", {name: "Draft actions"})).toBeTruthy();
});
