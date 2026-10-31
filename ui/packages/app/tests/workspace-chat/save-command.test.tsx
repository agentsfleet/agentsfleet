import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { CHAT_PROPS, ChatFixture, chatRouter } from "./fixture";
import { WorkspaceChat } from "@/components/domain/chat/WorkspaceChat";
import { DRAFT_EVENT } from "@/lib/api/draft-events";
import { REQUEST_METHOD } from "@/lib/api/draft-request";
import { DRAFT_STATE, MESSAGE_ROLE, MESSAGE_STATE, type Message } from "@/lib/api/draft-types";
import { DRAFT_ID, FLEET, FLEET_ID, MESSAGE, REVISION_ID, response } from "@/tests/helpers/draft-fixtures";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";

const SAVED = {...FLEET, setup_revision: "3", status: "active", setup: {state: "ready", checks: []}};
const SAVE = "Save";
const SAVING = "Saving";
const ACKNOWLEDGEMENT = "Setup saved · v3";
const MESSAGES = "/messages";
const FLEETS = "/fleets";

class SaveChatFixture extends ChatFixture {
  messageResponse: Promise<Response> | undefined;
  constructor() {
    super();
    this.messages = [MESSAGE, {...MESSAGE, role: MESSAGE_ROLE.assistant, text: "Which apps should it watch?"}];
    const route = this.fetch.getMockImplementation()!;
    this.fetch.mockImplementation(async (input, init) => {
      const path = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, "http://app.test").pathname;
      if (path.endsWith(`/fleets/${FLEET_ID}`)) return response(SAVED);
      if (init?.method !== REQUEST_METHOD.post || !path.endsWith(MESSAGES)) return route(input, init);
      if (typeof init.body !== "string") throw new Error("Expected a JSON request body");
      const body = JSON.parse(init.body);
      this.calls.push({path, method: init.method, body, headers: new Headers(init.headers)});
      if (this.messageResponse) return this.messageResponse;
      const user: Message = {...MESSAGE, sequence: "2", operation_id: body.operation_id, text: body.message, save_requested: body.save};
      this.messages = [...this.messages, user, {...user, role: MESSAGE_ROLE.assistant, state: MESSAGE_STATE.running, text: undefined}];
      return response({status: "accepted", sequence: "2", replayed: false}, 202);
    });
  }
  finishModel(state: Message["state"] = MESSAGE_STATE.completed, refused = false): void {
    this.draft = {...this.draft, candidate: {...this.draft.candidate!, id: REVISION_ID},
      review: {...this.draft.review!, review_digest: "finalized-review"}};
    this.messages = this.messages.map(message => message.sequence === "2" && message.role === MESSAGE_ROLE.assistant
      ? {...message, state, candidate_id: REVISION_ID, text: "The instructions now include your latest decisions."} : message);
    if (state === MESSAGE_STATE.completed && this.messages.at(-1)?.save_requested) {
      if (refused) this.messages = this.messages.map(message => message.role === MESSAGE_ROLE.assistant && message.sequence === "2"
        ? {...message, save_error_code: "AGENTSFLEET_NAME_EXISTS"} : message);
      else this.draft = {...this.draft, state: DRAFT_STATE.closed, linked_fleet_id: FLEET_ID};
    }
    FakeEventSource.instances.at(-1)!.emitNamed(DRAFT_EVENT.completed, JSON.stringify({draft_id: DRAFT_ID}));
  }
}

let fixture: SaveChatFixture;
beforeEach(() => { fixture = new SaveChatFixture(); fixture.mount(); });
afterEach(() => { cleanup(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

async function requestSave(): Promise<void> {
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT_ID} />);
  await screen.findByText("Which apps should it watch?");
  const input = screen.getByRole("textbox");
  await waitFor(() => expect(input).toHaveProperty("disabled", false));
  fireEvent.change(input, {target: {value: SAVE}});
  fireEvent.click(screen.getByRole("button", {name: "Send message"}));
  await screen.findByText(SAVING, {exact: true});
}

it("shows saving through model finalization and confirms only after the write succeeds", async () => {
  await requestSave();
  expect(fixture.saveAttempts).toBe(0);
  expect(screen.queryByText(ACKNOWLEDGEMENT)).toBeNull();
  await act(async () => fixture.finishModel());
  expect(fixture.writes().find(call => call.path.endsWith(MESSAGES))?.body).toMatchObject({save: true});
  expect(fixture.writes().some(call => call.path.endsWith(FLEETS))).toBe(false);
  await screen.findByRole("heading", {name: ACKNOWLEDGEMENT});
  await waitFor(() => expect(screen.queryByText(SAVING, {exact: true})).toBeNull());
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByRole("button", {name: "Review setup"})).toBeNull();
  expect(screen.getByRole("textbox")).toHaveProperty("disabled", false);
  expect(chatRouter.push).not.toHaveBeenCalled();
});

it("ends progress and preserves chat when the database refuses the save", async () => {
  await requestSave();
  await act(async () => fixture.finishModel(MESSAGE_STATE.completed, true));
  await waitFor(() => expect(screen.queryByText(SAVING, {exact: true})).toBeNull());
  expect(screen.queryByText(ACKNOWLEDGEMENT)).toBeNull();
  expect(screen.getByText(SAVE, {exact: true})).toBeTruthy();
  expect(screen.getByRole("alert")).toBeTruthy();
  expect(fixture.saveAttempts).toBe(0);
});

it("does not write a setup when the model turn fails", async () => {
  await requestSave();
  await act(async () => fixture.finishModel(MESSAGE_STATE.failed));
  await waitFor(() => expect(screen.queryByText(SAVING, {exact: true})).toBeNull());
  expect(fixture.saveAttempts).toBe(0);
  expect(screen.queryByText(ACKNOWLEDGEMENT)).toBeNull();
  expect(screen.getByText(SAVE, {exact: true})).toBeTruthy();
});

it("keeps chatting available after the save message is refused", async () => {
  const admission = Promise.withResolvers<Response>();
  fixture.messageResponse = admission.promise;
  await requestSave();
  await act(async () => admission.resolve(response({}, 409)));
  await screen.findByRole("alert");
  await waitFor(() => expect(screen.queryByText(SAVING, {exact: true})).toBeNull());
  fireEvent.change(screen.getByRole("textbox"), {target: {value: "Which apps can it watch?"}});
  expect(screen.getByRole("button", {name: "Send message"})).toHaveProperty("disabled", false);
  fixture.messageResponse = undefined;
  fireEvent.click(screen.getByRole("button", {name: "Send message"}));
  await screen.findByText("Which apps can it watch?");
  await act(async () => fixture.finishModel());
  await screen.findByText("The instructions now include your latest decisions.");
  expect(fixture.saveAttempts).toBe(0);
  expect(screen.queryByText(ACKNOWLEDGEMENT)).toBeNull();
});
