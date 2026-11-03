import type { ThreadMessageLike } from "@assistant-ui/react";
import { AssistantRuntimeProvider, useExternalStoreRuntime } from "@assistant-ui/react";
import { FleetThreadViewport } from "@/components/domain/FleetThreadViewport";
import { TaskPanel } from "@/components/domain/chat/TaskPanel";
import { CONNECTION_STATUS } from "@/components/domain/useFleetEventStream";

const MESSAGES: ThreadMessageLike[] = [
  {id: "operator", role: "user", content: "Repair these three Linkwarden pull requests. Keep the regression checks."},
  {id: "operator:reply", role: "assistant", content: "The login repair is waiting for checks. I’m working on duplicate bookmarks next; the import repair is queued."},
];

/** Production chat components; Playwright supplies only the task HTTP boundary. */
export function TaskFixture() {
  const runtime = useExternalStoreRuntime<ThreadMessageLike>({messages: MESSAGES,
    convertMessage: message => message, onNew: async () => {}});
  return <main className="flex h-dvh flex-col bg-background text-foreground">
    <header className="shrink-0 border-b border-border px-xl py-lg"><h1 className="font-semibold">Linkwarden repairs</h1></header>
    <AssistantRuntimeProvider runtime={runtime}>
      <FleetThreadViewport eventsCount={2} connectionStatus={CONNECTION_STATUS.LIVE}
        work={<TaskPanel workspaceId="workspace" fleetId="linkwarden" subject="fixture-owner" revision={0} />}
        pending={[]} onRetry={() => {}} onResend={() => {}} onDismiss={() => {}}
        onRestored={() => {}} onDraft={() => {}} modelLabel="Selected coding model" />
    </AssistantRuntimeProvider>
  </main>;
}
