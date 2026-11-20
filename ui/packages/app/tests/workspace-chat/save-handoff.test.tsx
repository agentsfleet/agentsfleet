import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { saveInChat, ChatFixture, CHAT_PROPS, chatRouter } from "./fixture";
import { WorkspaceChat } from "@/components/domain/setup/conversation/WorkspaceChat";
import { CHAT_PLACEHOLDER } from "@/components/domain/setup/chat-defaults";
import { FLEET_ID, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
beforeEach(() => {fixture = new ChatFixture(); fixture.mount();});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});
async function proposed() {
  render(<WorkspaceChat {...CHAT_PROPS} />);
  const input = await screen.findByLabelText(CHAT_PLACEHOLDER);
  await waitFor(() => expect(input).toHaveProperty("disabled", false));
  fireEvent.change(input, {target: {value: "Watch releases; require my approval before each release."}});
  fireEvent.click(screen.getByRole("button", {name: /^(Send message|New fleet from message)$/}));
  await screen.findByText("Your setup is ready.");
}
function followupFailure(status: number) {
  const base = fixture.fetch.getMockImplementation();
  if (!base) throw new Error("Missing fixture transport");
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return fixture.saveAttempts > 0 && path.endsWith(`/drafts/${fixture.draft.id}`)
      ? response({}, status) : base(input, init);
  });
}

it("does not navigate or send runtime instructions for a preview", async () => {
  await proposed();
  expect(chatRouter.replace).not.toHaveBeenCalled();
  expect(fixture.saveAttempts).toBe(0);
  expect(fixture.writes().filter(call => call.path.includes("/fleets/"))).toEqual([]);
});

it("retains the save identity after a lost response and hands off once confirmation arrives", async () => {
  fixture.failSave = true; await proposed();
  await saveInChat();
  await screen.findAllByText("The request could not be confirmed. Retry the same operation.");
  expect(chatRouter.replace).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", {name: "Retry"}));
  await waitFor(() => expect(chatRouter.replace).toHaveBeenCalledExactlyOnceWith(`/w/${WORKSPACE}/fleets/${FLEET_ID}`, {scroll: false}));
  const saves = fixture.writes().filter(call => call.path.endsWith("/messages"));
  expect(saves).toHaveLength(2);
  expect(saves[0]?.body).toMatchObject({save: true, operation_id: expect.any(String)});
  expect(saves[0]?.body).toEqual(saves[1]?.body);
  expect(fixture.writes().filter(call => call.path.includes("/fleets/") && call.path.endsWith("/messages"))).toEqual([]);
});

it("waits for a recorded native save receipt when its following history read fails", async () => {
  followupFailure(503); await proposed(); await saveInChat();
  await screen.findByRole("alert", {name: "Chat recovery"});
  expect(chatRouter.replace).not.toHaveBeenCalled();
  expect(fixture.saveAttempts).toBe(1);
  expect(fixture.writes().filter(call => call.path.includes("/fleets/") && call.path.endsWith("/messages"))).toEqual([]);
});

it("blocks handoff when a follow-up read reports lost authorization", async () => {
  followupFailure(403); await proposed(); await saveInChat();
  await screen.findByText("Chat is unavailable");
  expect(fixture.saveAttempts).toBe(1);
  expect(chatRouter.replace).not.toHaveBeenCalled();
});
