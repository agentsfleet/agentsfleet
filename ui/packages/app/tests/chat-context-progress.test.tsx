import type { ThreadMessageLike } from "@assistant-ui/react";
import { AssistantRuntimeProvider, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";
import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { DraftMessage } from "@/components/domain/chat/DraftMessage";
import { CONTEXT_STATE } from "@/lib/context-progress";

afterEach(cleanup);
const start = {generation: 1, state: CONTEXT_STATE.summarizing};
function Conversation({value, running = true, text = ""}: {value: unknown; running?: boolean; text?: string}) {
  const messages: ThreadMessageLike[] = [{id: "private-summary", role: "assistant", content: [{type: "text", text}],
    status: running ? {type: "running"} : {type: "complete", reason: "stop"}, metadata: {custom: {contextProgress: value}}}];
  const runtime = useExternalStoreRuntime({messages, convertMessage: message => message, onNew: async () => {}});
  return <AssistantRuntimeProvider runtime={runtime}>
    <ThreadPrimitive.Messages>{({message}) => <DraftMessage message={message} />}</ThreadPrimitive.Messages>
  </AssistantRuntimeProvider>;
}

it("shows one private summary status without replacing an existing answer", () => {
  render(<Conversation value={start} text="Partial answer stays available" />);
  expect(screen.getAllByText("Context summarizing…")).toHaveLength(1);
  expect(screen.getByText("Partial answer stays available")).toBeTruthy();
  expect(screen.queryByRole("status", {name: "Working"})).toBeNull();
});

it("announces summarization before the first answer without a second working indicator", () => {
  render(<Conversation value={start} />);
  expect(screen.getAllByText("Context summarizing…")).toHaveLength(1);
  expect(screen.getByRole("status").getAttribute("aria-live")).toBe("polite");
  expect(screen.queryByRole("status", {name: "Working"})).toBeNull();
});

it("ignores stale summary metadata on a durable completed answer", () => {
  render(<Conversation value={start} running={false} text="Saved answer" />);
  expect(screen.getByText("Saved answer")).toBeTruthy();
  expect(screen.queryByText("Context summarizing…")).toBeNull();
});

it("retains working feedback when malformed metadata is received", () => {
  render(<Conversation value={{...start, notes: "private data"}} />);
  expect(screen.getByRole("status", {name: "Working"})).toBeTruthy();
  expect(screen.queryByText("Context summarizing…")).toBeNull();
  expect(screen.queryByText("private data")).toBeNull();
});
