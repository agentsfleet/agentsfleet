import { AssistantRuntimeProvider, ComposerPrimitive, ThreadPrimitive, useExternalStoreRuntime, type AppendMessage, type ThreadMessageLike } from "@assistant-ui/react";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { chatMessageText } from "@/lib/chat-message-text";
import { ComposerQuote, QuoteReply } from "./ChatQuote";

afterEach(cleanup);
const ANSWER = "Run the scans before publishing.";
const MESSAGES: ThreadMessageLike[] = [{id: "answer", role: "assistant", content: ANSWER}];
function Chat({onNew, disabled = false}: {onNew: (message: AppendMessage) => Promise<void>; disabled?: boolean}) {
  const runtime = useExternalStoreRuntime({messages: MESSAGES, isRunning: false, isDisabled: disabled, onNew, convertMessage: (message: ThreadMessageLike) => message});
  return <AssistantRuntimeProvider runtime={runtime}>
    <ThreadPrimitive.Messages components={{Message: () => <QuoteReply text={ANSWER} />}} />
    <ComposerPrimitive.Root><ComposerQuote /><ComposerPrimitive.Input aria-label="Message" /><ComposerPrimitive.Send>Send</ComposerPrimitive.Send></ComposerPrimitive.Root>
  </AssistantRuntimeProvider>;
}

it("uses the native composer quote state for preview, dismissal and the submitted message", async () => {
  const sent: string[] = [];
  render(<Chat onNew={async message => {sent.push(chatMessageText(message));}} />);
  expect(screen.queryByRole("button", {name: "Remove quote"})).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Quote reply"}));
  expect(await screen.findByText(ANSWER)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Remove quote"}));
  expect(screen.queryByText(ANSWER)).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Quote reply"}));
  fireEvent.change(screen.getByRole("textbox", {name: "Message"}), {target: {value: "Which scans?"}});
  await act(async () => {fireEvent.click(screen.getByRole("button", {name: "Send"}));});
  await waitFor(() => expect(sent).toEqual([`> ${ANSWER}\n\nWhich scans?`]));
  expect(screen.queryByRole("button", {name: "Remove quote"})).toBeNull();
});

it("does not offer a quote action on a disabled thread", () => {
  const send = vi.fn();
  render(<Chat disabled onNew={send} />);
  expect(screen.getByRole("button", {name: "Quote reply"}).hasAttribute("disabled")).toBe(true);
  fireEvent.click(screen.getByRole("button", {name: "Quote reply"}));
  expect(screen.queryByRole("button", {name: "Remove quote"})).toBeNull();
  expect(send).not.toHaveBeenCalled();
});
