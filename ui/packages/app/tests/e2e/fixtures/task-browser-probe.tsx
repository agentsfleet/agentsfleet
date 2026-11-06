import type { ThreadMessageLike } from "@assistant-ui/react";
import { RENDER_KIND_KEY, RENDER_KIND } from "@/components/domain/useFleetThreadEntries";
import { ACTOR } from "@/lib/events/event-summary";
import { SenderLabelProvider } from "@/components/domain/FleetMessageRow";
import { TASK_IDS } from "@/tests/helpers/task-reads";
import { FleetTaskProvider } from "@/components/domain/chat/FleetTaskProvider";
import { AssistantRuntimeProvider, useExternalStoreRuntime } from "@assistant-ui/react";
import { FleetThreadViewport } from "@/components/domain/FleetThreadViewport";
import { FleetConversationLayout } from "@/components/domain/FleetWorkSidebar";
import { TaskPanel } from "@/components/domain/chat/TaskPanel";
import { CONNECTION_STATUS } from "@/components/domain/useFleetEventStream";

const TASK_ACTIVITY = {items: [{kind: "task_transfer_progress", progress: {activity_id: TASK_IDS[0], task_id: TASK_IDS[0], source_admission_id: null, operation: "capture", phase: "saved", provider: null, model: null, completed: 1, total: 1}}], truncated: false};
const MESSAGES: ThreadMessageLike[] = [
  {id: "operator", role: "user", content: "Repair these three Linkwarden pull requests. Keep the regression checks.",
    metadata: {custom: {[RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, actor: `${ACTOR.STEER_PREFIX}fixture-owner`}}},
  {id: "operator:reply", role: "assistant", content: "The login repair is waiting for checks. I’m working on duplicate bookmarks next; the import repair is queued.", metadata: {custom: {taskActivity: TASK_ACTIVITY}}},
];

/** Production chat components; Playwright supplies only the task HTTP boundary. */
export function TaskFixture() {
  const runtime = useExternalStoreRuntime<ThreadMessageLike>({messages: MESSAGES,
    convertMessage: message => message, onNew: async () => {}});
  return <main className="flex h-dvh flex-col bg-background text-foreground">
    <header className="shrink-0 border-b border-border px-xl py-lg"><h1 className="font-semibold">Linkwarden repairs</h1></header>
    <AssistantRuntimeProvider runtime={runtime}><SenderLabelProvider senderLabel="Linkwarden repair fleet across connected repositories">
      <FleetTaskProvider workspaceId="workspace" fleetId="linkwarden" subject="fixture-owner" revision={0} sources={{repositories: ["linkwarden/linkwarden"], access: "write", publication: "existing_pr"}}>
      <FleetConversationLayout work={<TaskPanel workspaceId="workspace" fleetId="linkwarden" subject="fixture-owner" revision={0} sources={{repositories: ["linkwarden/linkwarden"], access: "write", publication: "existing_pr"}} />}><FleetThreadViewport eventsCount={2} connectionStatus={CONNECTION_STATUS.LIVE}
        pending={[]} onRetry={() => {}} onResend={() => {}} onDismiss={() => {}}
        onRestored={() => {}} onDraft={() => {}} modelLabel="Selected coding model" /></FleetConversationLayout>
      </FleetTaskProvider>
    </SenderLabelProvider></AssistantRuntimeProvider>
  </main>;
}
