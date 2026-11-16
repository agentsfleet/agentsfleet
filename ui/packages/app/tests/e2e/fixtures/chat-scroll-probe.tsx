import type { AppendMessage, ThreadMessageLike } from "@assistant-ui/react";
import { RENDER_KIND_KEY, RENDER_KIND } from "@/components/domain/conversation/thread/useFleetThreadEntries";
import { ACTOR } from "@/lib/events/event-summary";
import { chatMessageText } from "@/lib/chat-message-text";
import { AssistantRuntimeProvider, useExternalStoreRuntime } from "@assistant-ui/react";
import { useEffect, useState } from "react";
import { FleetThreadViewport } from "@/components/domain/conversation/thread/FleetThreadViewport";
import { CONNECTION_STATUS } from "@/components/domain/conversation/stream/useFleetEventStream";

const OWNER = {[RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, actor: `${ACTOR.STEER_PREFIX}fixture-owner`};
// `?tall=17,18` makes those answers taller than a settled row's layout placeholder.
const TALL = new Set((new URLSearchParams(location.search).get("tall") ?? "").split(","));
const HISTORY: ThreadMessageLike[] = Array.from({length: 20}, (_, index) => [
  {id: `question-${index}`, role: "user" as const, content: `Explain repair step ${index + 1}.`, metadata: {custom: OWNER}},
  {id: `answer-${index}`, role: "assistant" as const, content: TALL.has(String(index))
    ? Array.from({length: 30}, (_, line) => `Step ${index + 1} evidence ${line + 1}: the callback repair holds.`).join("\n\n")
    : `Recorded progress ${index + 1}. The callback repair is being checked against the saved regression.`},
]).flat();
const REPLY_LINE = "Streamed evidence: the callback regression is still running.\n\n";

/** Exercise real chat scrolling while only the incoming event boundary is simulated. */
export function ChatScrollFixture() {
  const [messages, setMessages] = useState(HISTORY);
  const [running, setRunning] = useState(false);
  useEffect(() => {
    const receive = () => setMessages(previous => [...previous, {
      id: `progress-${previous.length}`, role: "assistant",
      content: "New recorded update: the callback regression is still running. Its result has not been confirmed.",
    }]);
    // One streamed chunk onto the reply this tab's send opened.
    const chunk = () => setMessages(previous => previous.map((message, index) =>
      index === previous.length - 1 && typeof message.content === "string" ? {...message, content: `${message.content}${REPLY_LINE}`} : message));
    window.addEventListener("fixture:progress", receive);
    window.addEventListener("fixture:chunk", chunk);
    return () => {
      window.removeEventListener("fixture:progress", receive);
      window.removeEventListener("fixture:chunk", chunk);
    };
  }, []);
  const runtime = useExternalStoreRuntime({messages, isRunning: running,
    convertMessage: (message: ThreadMessageLike) => message, onNew: async (message: AppendMessage) => {
      setMessages(previous => [...previous,
        {id: `sent-${previous.length}`, role: "user", content: chatMessageText(message), metadata: {custom: OWNER}},
        {id: `sent-${previous.length}:reply`, role: "assistant", content: "", status: {type: "running"}}]);
      setRunning(true);
    }});
  return <main className="flex h-dvh flex-col bg-background text-foreground">
    <AssistantRuntimeProvider runtime={runtime}>
      <FleetThreadViewport eventsCount={messages.length} connectionStatus={CONNECTION_STATUS.LIVE}
        pending={[]} onRetry={() => {}} onResend={() => {}} onDismiss={() => {}}
        onRestored={() => {}} onDraft={() => {}} />
    </AssistantRuntimeProvider>
  </main>;
}
