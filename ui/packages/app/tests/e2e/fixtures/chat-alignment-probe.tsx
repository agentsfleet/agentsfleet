import { DraftConversation } from "@/components/domain/setup/conversation/DraftConversation";
import { DraftChatHeading } from "@/components/domain/setup/conversation/DraftChatHeading";
import { DraftClient } from "@/lib/api/drafts";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { useState, type ReactNode } from "react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { ShellFrame } from "@/components/layout/ShellFrame";
import { OWN_ACCOUNT } from "@/tests/helpers/workspace-fixtures";
import { PathnameContext, SearchParamsContext } from "next/dist/shared/lib/hooks-client-context.shared-runtime";
import { SenderNamesProvider } from "@/components/domain/shared/identity/SenderNames";
import { SenderLabelProvider } from "@/components/domain/activity/messages/FleetMessageRow";
import { senderNamesFrom } from "@/lib/events/sender-names";
import { AssistantRuntimeProvider, useExternalStoreRuntime, type ThreadMessageLike } from "@assistant-ui/react";
import { AppRouterContext } from "next/dist/shared/lib/app-router-context.shared-runtime";
import { FleetHeader } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetHeader";
import { FleetConversationLayout } from "@/components/domain/shared/summary/FleetWorkSidebar";
import { FleetThreadViewport } from "@/components/domain/conversation/thread/FleetThreadViewport";
import { TaskPanel } from "@/components/domain/tasks/TaskPanel";
import { CONNECTION_STATUS } from "@/components/domain/conversation/stream/useFleetEventStream";
import { RENDER_KIND, RENDER_KIND_KEY } from "@/components/domain/conversation/thread/useFleetThreadEntries";
import { ACTOR } from "@/lib/events/event-summary";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { FLEET_DETAIL, WORKSPACE, FLEET_ID, FLEET, DRAFT, MODEL, MESSAGE, REVIEW } from "@/tests/helpers/draft-fixtures";
import { SCHEDULE_RUN_STATE, SCHEDULE_SYNC } from "@/lib/api/draft-vocabulary";
import { NANOS_PER_USD } from "@/lib/types";
import { WorkspaceChat } from "@/components/domain/setup/conversation/WorkspaceChat";

const router = {bfcacheId: "visual-fixture", back() {}, forward() {}, refresh() {}, push() {}, replace() {}, async prefetch() {}};
const exchanges = [
  ["What is happening with the Linkwarden repair?", "The selected run failed its callback regression. I’m inspecting the recorded failure before changing the source."],
  ["Use that run and attempt. Keep the existing assertions.", "I’ll keep the repair within the reviewed repository and preserve the regression assertions."],
  ["Require passing checks before marking the repair done.", "The completion criterion will require passing checks on the proposed revision. Jev evaluates the recorded evidence against that criterion."],
  ["Save those instructions.", "In this demonstration, the saved setup receipt is simulated. Production confirms a save only after the existing API accepts it."],
  ["Show me the durable task and its progress.", "Summary shows the saved objective, criteria and recorded progress. You can keep chatting while the fleet works."],
];
const history: ThreadMessageLike[] = exchanges.flatMap(([question, answer], index) => [
  {id: `operator-${index}`, role: "user", content: question ?? "", metadata: {custom: {[RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, actor: `${ACTOR.STEER_PREFIX}fixture-owner`}}},
  {id: `reply-${index}`, role: "assistant", content: answer ?? ""},
]);

/** Production header, transcript, composer and task reader; only API data is simulated. */
export function ChatAlignmentFixture() {
  const [messages, setMessages] = useState(history);
  const runtime = useExternalStoreRuntime({messages, convertMessage: (message: ThreadMessageLike) => message,
    onNew: async message => {setMessages(previous => [...previous, {id: `sent-${previous.length}`, role: "user", content: message.content,
      metadata: {custom: {[RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, actor: `${ACTOR.STEER_PREFIX}fixture-owner`}}}]);}});
  return <AppRouterContext value={router}><ChatFixtureShell>
    <FleetHeader workspaceId={WORKSPACE} fleet={FleetDetailSchema.parse({...FLEET_DETAIL, name: "ci-repairer", status: "active"})} />
    <AssistantRuntimeProvider runtime={runtime}><FleetConversationLayout work={<TaskPanel workspaceId={WORKSPACE} fleetId={FLEET_ID} subject="fixture-owner" revision={0} sources={{repositories: ["agentsfleet/linkwarden"], access: "write", publication: "existing_pr"}} />}>
      <FleetThreadViewport modelLabel="MiniMax M3.1 Flash" eventsCount={messages.length} connectionStatus={CONNECTION_STATUS.LIVE}
        pending={[]} onRetry={() => {}} onResend={() => {}} onDismiss={() => {}} onRestored={() => {}} onDraft={() => {}} />
    </FleetConversationLayout></AssistantRuntimeProvider>
  </ChatFixtureShell></AppRouterContext>;
}


const setupClient = new DraftClient({account: "fixture-owner", workspace: WORKSPACE, subject: "creator"});
export function ChatErrorFixture() {
  return <AppRouterContext value={router}><ChatFixtureShell setup>
    <WorkspaceChat account="fixture-owner" workspace={WORKSPACE} />
  </ChatFixtureShell></AppRouterContext>;
}

export function SetupAlignmentFixture() {
  const model = {...MODEL, label: "MiniMax M3.1 Flash"};
  const definition = {...REVIEW.effective.definition, config: {...REVIEW.effective.definition.config,
    "x-agentsfleet": {triggers: [{type: "cron", schedule: "0 9 * * 1-5", timezone: "Asia/Kolkata"}]}}};
  const [createdAt] = useState(() => new Date(Date.now() - 5 * 60 * 1000).toISOString());
  const view = {...EMPTY_DRAFT_VIEW, draft: {...DRAFT, candidate: DRAFT.candidate ? {...DRAFT.candidate, definition} : undefined, linked_fleet_id: FLEET_ID, review: {...REVIEW,
    requirements: {credentials: ["github"], repositories: ["agentsfleet/linkwarden"], tools: ["shell"],
      network: {http_egress: "restricted", allow: ["api.github.com"]}, budget: {daily_nanos: String(NANOS_PER_USD)}},
    effective: {...REVIEW.effective, definition, instance_name: "Pull request review", target_setup_revision: "1"}}}, fleet: {...FLEET, name: "Pull request review", schedules: [{schedule_id: "review-mornings", fleet_id: FLEET_ID, description: "Weekday pull request review", cron: "0 9 * * 1-5", timezone: "Asia/Kolkata", message: "Review open pull requests", status: "active", sync: SCHEDULE_SYNC.failed, last_error: "Scheduler unavailable", run_state: SCHEDULE_RUN_STATE.unavailable, next_run_at: null}]}, model: model.ref, models: [model],
    messages: exchanges.flatMap(([question, answer], index) => [
      {...MESSAGE, created_at: createdAt, sequence: String(index * 2 + 1), text: question ?? ""},
      {...MESSAGE, created_at: createdAt, sequence: String(index * 2 + 2), role: "assistant" as const, text: answer ?? ""},
    ])};
  return <AppRouterContext value={router}><ChatFixtureShell setup>
    <DraftChatHeading client={setupClient} view={view} workspace={WORKSPACE} onDirtyChange={() => {}} />
      <DraftConversation client={setupClient} view={view} workspace={WORKSPACE} />
  </ChatFixtureShell></AppRouterContext>;
}

/** Render the production shell; only identity and API data are supplied by fixtures. */
function ChatFixtureShell({children, setup = false}: {children: ReactNode; setup?: boolean}) {
  return <PathnameContext value={`/w/${WORKSPACE}/fleets/${setup ? "new" : FLEET_ID}`}><SearchParamsContext value={new URLSearchParams()}>
    <TooltipProvider><ShellFrame workspaces={[{id: WORKSPACE, name: "Release workspace", created_at: "2026-01-01T00:00:00Z", ...OWN_ACCOUNT}]}>
      <SenderLabelProvider senderLabel="ci-repairer"><SenderNamesProvider names={senderNamesFrom("fixture-owner", [])}>
        <div data-fleet-workspace className="flex h-full min-h-0 flex-col">{children}</div>
      </SenderNamesProvider></SenderLabelProvider>
    </ShellFrame></TooltipProvider>
  </SearchParamsContext></PathnameContext>;
}
