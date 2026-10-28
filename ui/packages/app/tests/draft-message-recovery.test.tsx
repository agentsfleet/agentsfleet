import type { ThreadMessageLike } from "@assistant-ui/react";
import { AssistantRuntimeProvider, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { DraftMessage } from "@/components/domain/chat/DraftMessage";
import { DraftConversation } from "@/components/domain/chat/DraftConversation";
import { DraftClient } from "@/lib/api/drafts";
import { ChatFixture, CHAT_PROPS } from "./workspace-chat/fixture";
import { DRAFT, MESSAGE, response } from "./helpers/draft-fixtures";

const RETRY = "Try again";
const PARTIAL = "The partial answer is still available.";
let fixture: ChatFixture;
let client: DraftClient;
beforeEach(async () => {
  fixture = new ChatFixture(); fixture.mount();
  client = new DraftClient({...CHAT_PROPS, subject: "creator"});
  await client.initialize();
});
afterEach(() => {cleanup(); client.dispose(); vi.unstubAllGlobals();});

function RecordedFailure({failure}: {failure: unknown}) {
  const messages: ThreadMessageLike[] = [{id: "recorded-failure", role: "assistant", content: [{type: "text", text: PARTIAL}],
    status: {type: "complete", reason: "stop"}, metadata: {custom: {failure, retryText: MESSAGE.text}}}];
  const runtime = useExternalStoreRuntime({messages, convertMessage: message => message,
    onNew: async () => {throw new Error("Recorded failure replay cannot send new requests.");}});
  return <AssistantRuntimeProvider runtime={runtime}>
    <ThreadPrimitive.Messages>{({message}) => <DraftMessage message={message} />}</ThreadPrimitive.Messages>
  </AssistantRuntimeProvider>;
}

it("preserves a partial answer but refuses malformed failure metadata", () => {
  render(<RecordedFailure failure={{error_code: 42}} />);
  expect(screen.getByText(PARTIAL)).toBeTruthy();
  expect(screen.queryByRole("button", {name: RETRY})).toBeNull();
  expect(screen.queryByText("Couldn’t complete this request")).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it.each([
  {cause: "deadline", title: "Model request timed out"},
  {cause: "cancelled", title: "Request stopped"},
])("explains a recorded $cause without retrying outside its owning conversation", ({cause, title}) => {
  render(<RecordedFailure failure={{reason: "provider_failed", cause, error_code: "UZ-AUTHOR-014", recovery_action: "retry_message"}} />);
  expect(screen.getByText(title)).toBeTruthy();
  expect(screen.getByText(PARTIAL)).toBeTruthy();
  expect(screen.queryByRole("button", {name: RETRY})).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it("rechecks the client before admitting a retry from a stale eligible render", async () => {
  const failure = {reason: "provider_failed", error_code: "UZ-AUTHOR-014", recovery_action: "retry_message"};
  fixture.messages = [{...MESSAGE, state: "failed"}, {...MESSAGE, role: "assistant", state: "failed", failure}];
  await client.resume(DRAFT.id);
  render(<DraftConversation workspace={CHAT_PROPS.workspace} client={client} view={client.getSnapshot()} />);
  const pending = Promise.withResolvers<Response>();
  fixture.fetch.mockImplementationOnce(() => pending.promise);
  const renaming = client.rename("Another mutation owns this conversation");
  expect(client.getSnapshot().busy).toBe(true);
  const writes = fixture.writes().length;
  await act(async () => fireEvent.click(screen.getByRole("button", {name: RETRY})));
  expect(fixture.writes()).toHaveLength(writes);
  expect(screen.getByRole("log").textContent).toContain(MESSAGE.text);
  await act(async () => {pending.resolve(response({...fixture.draft, title: "Renamed setup"})); await renaming;});
  expect(client.getSnapshot().draft?.title).toBe("Renamed setup");
  expect(fixture.calls.some(call => call.path.endsWith("/messages") && call.method === "POST")).toBe(false);
});
