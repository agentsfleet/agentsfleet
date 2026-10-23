import type { DraftView } from "@/lib/api/draft-state";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS, chooseSetupAction, setupAction } from "./fixture";
import { DraftChatHeading } from "@/components/domain/chat/DraftChatHeading";
import { WorkspaceChat } from "@/components/domain/chat/WorkspaceChat";
import { CHAT_PLACEHOLDER } from "@/components/domain/chat/chat-defaults";
import { DraftClient } from "@/lib/api/drafts";
import { DRAFT, MESSAGE, REVIEW, response } from "@/tests/helpers/draft-fixtures";

const MENU = "Setup chat actions";
const EDIT = "Edit instructions";
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
  render(<DraftChatHeading client={client} view={{...client.getSnapshot(), draft: null, preview: null, inaccessible}} />);
  expect(screen.queryByRole("button", {name: MENU})).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it("offers editing before the first message without allowing an unreviewed save or rename", async () => {
  client.fresh({kind: "definition", definition: REVIEW.effective.definition}); await client.refreshSetup();
  render(<DraftChatHeading client={client} view={client.getSnapshot()} />);
  expect(screen.queryByRole("button", {name: SAVE})).toBeNull();
  await setupAction(EDIT);
  expect(screen.getByRole("menuitem", {name: RENAME}).getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(screen.getByRole("menuitem", {name: EDIT}));
  const dialog = await screen.findByRole("dialog", {name: EDIT});
  expect(dialog.className).toContain("ph-no-capture");
  expect(dialog.className).toContain("ph-no-record");
  expect(screen.getByLabelText("Instructions")).toHaveProperty("value", REVIEW.effective.definition.instructions_md);
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
  render(<DraftChatHeading client={client} view={{...client.getSnapshot(), ...change}} />);
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
  render(<DraftChatHeading client={client} view={{...client.getSnapshot(), draft: {...DRAFT, candidate: undefined, review: undefined}}} />);
  expect(screen.queryByRole("button", {name: SAVE})).toBeNull();
  await setupAction(EDIT);
  expect(screen.getByRole("menuitem", {name: EDIT}).getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(screen.getByRole("menuitem", {name: RENAME}));
  fireEvent.change(screen.getByLabelText("Chat name"), {target: {value: "  "}});
  expect(screen.getByRole("button", {name: "Save name"})).toHaveProperty("disabled", true);
  fireEvent.click(screen.getByRole("button", {name: "Save name"}));
  expect(fixture.writes()).toHaveLength(0);
});

it("allows editing after Stop has cleared the pending turn", async () => {
  render(<DraftChatHeading client={client} view={{...client.getSnapshot(), draft: {...DRAFT, cancelled_through: MESSAGE.sequence},
    messages: [{...MESSAGE, state: "running"}]}} />);
  expect((await setupAction(EDIT)).getAttribute("aria-disabled")).not.toBe("true");
  fireEvent.click(screen.getByRole("menuitem", {name: EDIT}));
  expect(await screen.findByRole("dialog", {name: EDIT})).toBeTruthy();
  expect(screen.getByRole("button", {name: "Update instructions"})).toHaveProperty("disabled", false);
});

it("retains the chat composer and failed instruction edits when the panel is opened and closed", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  const input = await screen.findByLabelText(CHAT_PLACEHOLDER);
  await waitFor(() => expect(input).toHaveProperty("disabled", false));
  fireEvent.change(input, {target: {value: "Keep this unsent message"}});
  await chooseSetupAction(EDIT);
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: "Keep these unsaved instructions"}});
  fixture.fetch.mockResolvedValueOnce(response({user_message: "These instructions changed elsewhere"}, 412));
  fireEvent.click(screen.getByRole("button", {name: "Update instructions"}));
  await screen.findAllByText("These instructions changed elsewhere");
  expect(screen.getByLabelText("Instructions")).toHaveProperty("value", "Keep these unsaved instructions");
  expect(input.isConnected).toBe(true);
  expect(input).toHaveProperty("value", "Keep this unsent message");
  fireEvent.click(screen.getByRole("button", {name: "Cancel editing"}));
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
  const instructions = screen.getByLabelText("Instructions");
  fireEvent.change(instructions, {target: {value: "Preserve these preview edits"}});
  fireEvent.click(screen.getByRole("button", {name: "Update instructions"}));
  await waitFor(() => expect(fixture.fetch.mock.calls.some(([, init]) => init?.method === "PATCH")).toBe(true));
  expect(instructions.isConnected).toBe(true);
  expect(screen.getByRole("dialog", {name: EDIT})).toBeTruthy();
  await act(async () => patch.resolve(response({user_message: "Instructions were refused"}, 422)));
  await screen.findAllByText("Instructions were refused");
  expect(screen.getByLabelText("Instructions")).toBe(instructions);
  expect(instructions).toHaveProperty("value", "Preserve these preview edits");
  expect(fixture.saveAttempts).toBe(0);
});

it("clears unfinished rename and editor state when New fleet resets the conversation", async () => {
  const snapshot = client.getSnapshot();
  const view = render(<DraftChatHeading client={client} view={snapshot} />);
  await chooseSetupAction(RENAME);
  fireEvent.change(screen.getByLabelText("Chat name"), {target: {value: "Abandoned name from the old chat"}});
  view.rerender(<DraftChatHeading client={client} view={{...snapshot, draft: null, preview: null}} />);
  expect(screen.queryByLabelText("Chat name")).toBeNull();
  view.rerender(<DraftChatHeading client={client} view={{...snapshot, draft: {...DRAFT, title: "The new conversation"}}} />);
  expect(screen.queryByLabelText("Chat name")).toBeNull();
  expect(screen.getByRole("heading", {name: "The new conversation"})).toBeTruthy();
  await chooseSetupAction(EDIT);
  view.rerender(<DraftChatHeading client={client} view={{...snapshot, draft: null, preview: null}} />);
  view.rerender(<DraftChatHeading client={client} view={snapshot} />);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});
