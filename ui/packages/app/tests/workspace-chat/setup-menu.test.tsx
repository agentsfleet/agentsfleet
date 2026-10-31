import type { DraftView } from "@/lib/api/draft-state";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS, chatRouter, chooseSetupAction, setupAction } from "./fixture";
import { DraftChatHeading } from "@/components/domain/chat/DraftChatHeading";
import { DraftSaveButton } from "@/components/domain/chat/DraftSaveBar";
import { WorkspaceChat } from "@/components/domain/chat/WorkspaceChat";
import { CHAT_PLACEHOLDER } from "@/components/domain/chat/chat-defaults";
import { DraftClient } from "@/lib/api/drafts";
import { DRAFT, FLEET, MESSAGE, REVIEW, response } from "@/tests/helpers/draft-fixtures";

const MENU = "Setup chat actions";
const EDIT = "Instructions";
const SAVE = "Save fleet";
const RENAME = "Rename chat";
let fixture: ChatFixture;
let client: DraftClient;
beforeEach(async () => {
  fixture = new ChatFixture(); fixture.mount();
  client = new DraftClient({...CHAT_PROPS, subject: "creator"});
  await client.initialize(); await client.resume(DRAFT.id);
});
afterEach(() => {cleanup(); client.dispose(); vi.unstubAllGlobals();});

it.each([false, true])("hides private actions without an accessible setup (inaccessible: %s)", (inaccessible) => {
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} onDirtyChange={vi.fn()} client={client} view={{...client.getSnapshot(), draft: null, preview: null, inaccessible}} />);
  expect(screen.queryByRole("button", {name: MENU})).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it("offers editing before the first message without allowing an unreviewed save or rename", async () => {
  client.fresh({kind: "definition", definition: REVIEW.effective.definition}); await client.refreshSetup();
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} onDirtyChange={vi.fn()} client={client} view={client.getSnapshot()} />);
  expect(screen.getByRole("button", {name: SAVE})).toHaveProperty("disabled", true);
  await setupAction(EDIT);
  expect(screen.getByRole("menuitem", {name: RENAME}).getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(screen.getByRole("menuitem", {name: EDIT}));
  const dialog = await screen.findByRole("dialog", {name: EDIT});
  expect(dialog.className).toContain("ph-no-capture");
  expect(dialog.className).toContain("ph-no-record");
  expect(screen.getByRole("textbox", {name: "Instructions"})).toHaveProperty("value", REVIEW.effective.definition.instructions_md);
  fireEvent.click(screen.getByRole("button", {name: "Close"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(document.activeElement).toBe(screen.getByRole("button", {name: MENU}));
  expect(fixture.writes()).toHaveLength(0);
});

it.each([
  {busy: true}, {pending: true},
  {messages: [{...MESSAGE, state: "queued" as const}]},
  {messages: [{...MESSAGE, state: "running" as const}]},
  {messages: [{...MESSAGE, role: "assistant" as const, state: "failed" as const,
    failure: {reason: "provider_usage_unknown", error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"}}]},
  {draft: {...DRAFT, state: "saving" as const}},
])("blocks mutations while the setup cannot be edited (%j)", async (change: Partial<DraftView>) => {
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} onDirtyChange={vi.fn()} client={client} view={{...client.getSnapshot(), ...change}} />);
  const save = screen.queryByRole("button", {name: SAVE});
  if (save) expect(save).toHaveProperty("disabled", true);
  await setupAction(EDIT);
  for (const name of [EDIT, RENAME]) {
    const action = screen.getByRole("menuitem", {name});
    expect(action.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(action);
  }
  expect(fixture.writes()).toHaveLength(0);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByLabelText("Chat name")).toBeNull();
});

it("keeps an unfinished candidate unsavable and allows naming the conversation", async () => {
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} onDirtyChange={vi.fn()} client={client} view={{...client.getSnapshot(), draft: {...DRAFT, candidate: undefined, review: undefined}}} />);
  expect(screen.getByRole("button", {name: SAVE})).toHaveProperty("disabled", true);
  await setupAction(EDIT);
  expect(screen.getByRole("menuitem", {name: EDIT}).getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(screen.getByRole("menuitem", {name: RENAME}));
  fireEvent.change(screen.getByLabelText("Chat name"), {target: {value: "  "}});
  expect(screen.getByRole("button", {name: "Save name"})).toHaveProperty("disabled", true);
  fireEvent.click(screen.getByRole("button", {name: "Save name"}));
  expect(fixture.writes()).toHaveLength(0);
});

it("allows editing after Stop has cleared the pending turn", async () => {
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} onDirtyChange={vi.fn()} client={client} view={{...client.getSnapshot(), draft: {...DRAFT, cancelled_through: MESSAGE.sequence},
    messages: [{...MESSAGE, state: "running"}]}} />);
  expect((await setupAction(EDIT)).getAttribute("aria-disabled")).not.toBe("true");
  fireEvent.click(screen.getByRole("menuitem", {name: EDIT}));
  expect(await screen.findByRole("dialog", {name: EDIT})).toBeTruthy();
  expect(screen.getByRole("button", {name: "Save changes"})).toHaveProperty("disabled", true);
});

it("retains the chat composer and failed instruction edits when the panel is opened and closed", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  const input = await screen.findByLabelText(CHAT_PLACEHOLDER);
  await waitFor(() => expect(input).toHaveProperty("disabled", false));
  fireEvent.change(input, {target: {value: "Keep this unsent message"}});
  await chooseSetupAction(EDIT);
  fireEvent.change(screen.getByRole("textbox", {name: "Instructions"}), {target: {value: "Keep these unsaved instructions"}});
  fixture.fetch.mockResolvedValueOnce(response({user_message: "These instructions changed elsewhere"}, 412));
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  await screen.findAllByText("These instructions changed elsewhere");
  expect(screen.getByRole("textbox", {name: "Instructions"})).toHaveProperty("value", "Keep these unsaved instructions");
  expect(input.isConnected).toBe(true);
  expect(input).toHaveProperty("value", "Keep this unsent message");
  fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(screen.getByLabelText(CHAT_PLACEHOLDER)).toBe(input);
  expect(input).toHaveProperty("value", "Keep this unsent message");
  expect(fixture.saveAttempts).toBe(0);
});

it("keeps preview edits open while draft creation succeeds and the instruction patch is rejected", async () => {
  const patch = Promise.withResolvers<Response>();
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation((input, init) => init?.method === "PATCH" ? patch.promise : base(input, init));
  render(<WorkspaceChat {...CHAT_PROPS} source={{kind: "definition", definition: REVIEW.effective.definition}} />);
  await chooseSetupAction(EDIT);
  const instructions = screen.getByRole("textbox", {name: "Instructions"});
  fireEvent.change(instructions, {target: {value: "Preserve these preview edits"}});
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  await waitFor(() => expect(fixture.fetch.mock.calls.some(([, init]) => init?.method === "PATCH")).toBe(true));
  expect(instructions.isConnected).toBe(true);
  expect(screen.getByRole("dialog", {name: EDIT})).toBeTruthy();
  await act(async () => patch.resolve(response({user_message: "Instructions were refused"}, 422)));
  await screen.findAllByText("Instructions were refused");
  expect(screen.getByRole("textbox", {name: "Instructions"})).toBe(instructions);
  expect(instructions).toHaveProperty("value", "Preserve these preview edits");
  expect(fixture.saveAttempts).toBe(0);
});

it("clears unfinished rename and editor state when New fleet resets the conversation", async () => {
  const snapshot = client.getSnapshot();
  const view = render(<DraftChatHeading workspace={CHAT_PROPS.workspace} onDirtyChange={vi.fn()} client={client} view={snapshot} />);
  await chooseSetupAction(RENAME);
  fireEvent.change(screen.getByLabelText("Chat name"), {target: {value: "Abandoned name from the old chat"}});
  view.rerender(<DraftChatHeading workspace={CHAT_PROPS.workspace} onDirtyChange={vi.fn()} client={client} view={{...snapshot, draft: null, preview: null}} />);
  expect(screen.queryByLabelText("Chat name")).toBeNull();
  view.rerender(<DraftChatHeading workspace={CHAT_PROPS.workspace} onDirtyChange={vi.fn()} client={client} view={{...snapshot, draft: {...DRAFT, title: "The new conversation"}}} />);
  expect(screen.queryByLabelText("Chat name")).toBeNull();
  expect(screen.getByRole("heading", {name: "The new conversation"})).toBeTruthy();
  await chooseSetupAction(EDIT);
  view.rerender(<DraftChatHeading workspace={CHAT_PROPS.workspace} onDirtyChange={vi.fn()} client={client} view={{...snapshot, draft: null, preview: null}} />);
  view.rerender(<DraftChatHeading workspace={CHAT_PROPS.workspace} onDirtyChange={vi.fn()} client={client} view={snapshot} />);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it("keeps one save action in the authoring heading", () => {
  const review = {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}};
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} onDirtyChange={vi.fn()} client={client} view={{...client.getSnapshot(), draft: {...DRAFT, review}}} />);
  expect(screen.getByRole("button", {name: "Save changes"})).toHaveProperty("disabled", false);
  expect(screen.queryByRole("button", {name: SAVE})).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

// Value: protects=instruction editing immediately after saving; fails_when=the saved chat cannot reopen editable current instructions;
// why_new=existing heading menu tests begin with unsaved drafts; seam=none; no_claim=database revision durability.
it("refreshes saved instructions before reopening their editor", async () => {
  await client.save();
  render(<DraftChatHeading workspace={CHAT_PROPS.workspace} onDirtyChange={vi.fn()} client={client} view={client.getSnapshot()} />);
  await chooseSetupAction(EDIT);
  expect(await screen.findByRole("textbox", {name: EDIT})).toHaveProperty("value", REVIEW.effective.definition.instructions_md);
  expect(fixture.saveAttempts).toBe(1);
});

it("removes the save action when the current viewer loses access", () => {
  render(<DraftSaveButton client={client} view={{...client.getSnapshot(), inaccessible: true}} />);
  expect(screen.queryByRole("button", {name: SAVE})).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

// Value: protects=opening the current fleet after activation; fails_when=the heading navigates to stale setup or writes a new revision;
// why_new=existing heading tests used no fleet or an installing fleet; seam=none; no_claim=server refresh success.
it("opens the current active fleet from the composer without writing a revision", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} fleetPage source={{kind: "fleet", fleet_id: FLEET.id, setup_revision: "1"}} />);
  const open = await screen.findByRole("button", {name: "Open fleet"});
  expect(screen.getByRole("heading", {name: FLEET.name})).toBeTruthy();
  expect(screen.queryByRole("link", {name: "Edit setup"})).toBeNull();
  expect(screen.getByTestId("fleet-chat-footer").contains(open)).toBe(true);
  fireEvent.click(open);
  expect(chatRouter.refresh).toHaveBeenCalledOnce();
  expect(chatRouter.push).not.toHaveBeenCalled();
  expect(fixture.writes()).toEqual([]);
});

// Value: protects=publishing a source before any chat turn; fails_when=publication opens without a reviewable draft or saves a fleet;
// why_new=existing publish tests began with a persisted draft; seam=none; no_claim=durable publication or provider latency.
it("prepares a definition source for publication without saving or running the fleet", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} source={{kind: "definition", definition: REVIEW.effective.definition}} />);
  await chooseSetupAction("Publish to Library");
  const label = await screen.findByLabelText("Library name");
  expect(label).toHaveProperty("value", REVIEW.effective.instance_name);
  expect(screen.getByRole("button", {name: "Publish new entry"})).toHaveProperty("disabled", false);
  expect(fixture.writes()).toHaveLength(1);
  expect(fixture.writes()[0]).toMatchObject({path: expect.stringMatching(/\/drafts$/), method: "POST",
    body: {source: {kind: "definition", definition: REVIEW.effective.definition}}});
  expect(fixture.saveAttempts).toBe(0);
});

// Value: protects=unsaved instruction changes on dialog dismissal; fails_when=declining discard loses manual work;
// why_new=existing cancel test skipped the dismissal confirmation; seam=none; no_claim=saved instruction durability.
it("keeps edited instructions when discard is refused and closes only after confirmation", async () => {
  const instructions = "Keep this manual change until I discard it";
  const confirm = vi.fn().mockReturnValueOnce(false).mockReturnValueOnce(true);
  vi.stubGlobal("confirm", confirm);
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  await chooseSetupAction(EDIT);
  fireEvent.change(screen.getByRole("textbox", {name: EDIT}), {target: {value: instructions}});
  fireEvent.click(screen.getByRole("button", {name: "Close"}));
  expect(screen.getByRole("textbox", {name: EDIT})).toHaveProperty("value", instructions);
  expect(screen.getByRole("dialog", {name: EDIT})).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Close"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(confirm).toHaveBeenCalledTimes(2);
  expect(fixture.writes()).toEqual([]);
});
