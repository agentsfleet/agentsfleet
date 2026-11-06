import { RENDER_KIND_KEY, RENDER_KIND } from "@/components/domain/useFleetThreadEntries";
import { ACTOR } from "@/lib/events/event-summary";
import type { ThreadMessageLike } from "@assistant-ui/react";
import { AssistantRuntimeProvider, useExternalStoreRuntime } from "@assistant-ui/react";
import { useEffect, useState } from "react";
import { FleetThreadViewport } from "@/components/domain/FleetThreadViewport";
import { CONNECTION_STATUS } from "@/components/domain/useFleetEventStream";

const HISTORY: ThreadMessageLike[] = Array.from({length: 20}, (_, index) => [
  {id: `question-${index}`, role: "user" as const, content: `Explain repair step ${index + 1}.`,
    metadata: {custom: {[RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, actor: `${ACTOR.STEER_PREFIX}fixture-owner`}}},
  {id: `answer-${index}`, role: "assistant" as const,
    content: `Recorded progress ${index + 1}. The callback repair is being checked against the saved regression.`},
]).flat();

/** Exercise real chat scrolling while only the incoming event boundary is simulated. */
export function ChatScrollFixture() {
  const [messages, setMessages] = useState(HISTORY);
  useEffect(() => {
    const receive = () => setMessages(previous => [...previous, {
      id: `progress-${previous.length}`, role: "assistant",
      content: "New recorded update: the callback regression is still running. Its result has not been confirmed.",
    }]);
    window.addEventListener("fixture:progress", receive);
    return () => window.removeEventListener("fixture:progress", receive);
  }, []);
  const runtime = useExternalStoreRuntime({messages,
    convertMessage: (message: ThreadMessageLike) => message, onNew: async () => {}});
  return <main className="flex h-dvh flex-col bg-background text-foreground">
    <AssistantRuntimeProvider runtime={runtime}>
      <FleetThreadViewport eventsCount={messages.length} connectionStatus={CONNECTION_STATUS.LIVE}
        pending={[]} onRetry={() => {}} onResend={() => {}} onDismiss={() => {}}
        onRestored={() => {}} onDraft={() => {}} />
    </AssistantRuntimeProvider>
  </main>;
}
