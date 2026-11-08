import { DraftConversation } from "@/components/domain/chat/DraftConversation";
import { DraftChatHeading } from "@/components/domain/chat/DraftChatHeading";
import { DraftClient } from "@/lib/api/drafts";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { useState, type ReactNode } from "react";
import { BrandMark, DashboardShellHeader } from "@agentsfleet/design-system";
import { PanelLeftCloseIcon, PlusIcon, PanelsTopLeftIcon } from "lucide-react";
import { AssistantRuntimeProvider, useExternalStoreRuntime, type ThreadMessageLike } from "@assistant-ui/react";
import { AppRouterContext } from "next/dist/shared/lib/app-router-context.shared-runtime";
import { ChatSummaryProvider } from "@/components/domain/chat/ChatSummary";
import { FleetHeader } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetHeader";
import { FleetConversationLayout } from "@/components/domain/FleetWorkSidebar";
import { FleetThreadViewport } from "@/components/domain/FleetThreadViewport";
import { TaskPanel } from "@/components/domain/chat/TaskPanel";
import { CONNECTION_STATUS } from "@/components/domain/useFleetEventStream";
import { RENDER_KIND, RENDER_KIND_KEY } from "@/components/domain/useFleetThreadEntries";
import { ACTOR } from "@/lib/events/event-summary";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { FLEET_DETAIL, WORKSPACE, FLEET_ID, FLEET, DRAFT, MODEL, MESSAGE, REVIEW } from "@/tests/helpers/draft-fixtures";

const router = {bfcacheId: "visual-fixture", back() {}, forward() {}, refresh() {}, push() {}, replace() {}, async prefetch() {}};
const exchanges = [
  ["What is happening with the Linkwarden repair?", "The selected run failed its callback regression. I’m inspecting the recorded failure before changing the source."],
  ["Use that run and attempt. Keep the existing assertions.", "I’ll keep the repair within the reviewed repository and preserve the regression assertions."],
  ["Require passing checks before marking the repair done.", "The completion criterion will require passing checks on the proposed revision. Jev evaluates the recorded evidence against that criterion."],
  ["Save those instructions.", "In this demonstration, the saved setup receipt is simulated. Production confirms a save only after the existing API accepts it."],
  ["Show me the durable task and its progress.", "Open Fleet summary beside the pencil. Durable tasks shows the saved objective, criteria and recorded progress. You can keep chatting while it runs."],
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
  return <AppRouterContext value={router}><ChatSummaryProvider><ChatFixtureShell>
    <FleetHeader workspaceId={WORKSPACE} fleet={FleetDetailSchema.parse({...FLEET_DETAIL, name: "ci-repairer", status: "active"})} />
    <AssistantRuntimeProvider runtime={runtime}><FleetConversationLayout work={<TaskPanel workspaceId={WORKSPACE} fleetId={FLEET_ID} subject="fixture-owner" revision={0} sources={{repositories: ["agentsfleet/linkwarden"], access: "write", publication: "existing_pr"}} />}>
      <FleetThreadViewport modelLabel="MiniMax M3.1 Flash" eventsCount={messages.length} connectionStatus={CONNECTION_STATUS.LIVE}
        pending={[]} onRetry={() => {}} onResend={() => {}} onDismiss={() => {}} onRestored={() => {}} onDraft={() => {}} />
    </FleetConversationLayout></AssistantRuntimeProvider>
  </ChatFixtureShell></ChatSummaryProvider></AppRouterContext>;
}


const setupClient = new DraftClient({account: "fixture-owner", workspace: WORKSPACE, subject: "creator"});
export function SetupAlignmentFixture() {
  const model = {...MODEL, label: "MiniMax M3.1 Flash"};
  const view = {...EMPTY_DRAFT_VIEW, draft: {...DRAFT, linked_fleet_id: FLEET_ID, review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}}, fleet: {...FLEET, name: "ci-repairer"}, model: model.ref, models: [model],
    messages: exchanges.flatMap(([question, answer], index) => [
      {...MESSAGE, sequence: String(index * 2 + 1), text: question ?? ""},
      {...MESSAGE, sequence: String(index * 2 + 2), role: "assistant" as const, text: answer ?? ""},
    ])};
  return <AppRouterContext value={router}><ChatSummaryProvider><ChatFixtureShell>
    <DraftChatHeading client={setupClient} view={view} workspace={WORKSPACE} onDirtyChange={() => {}} />
      <DraftConversation client={setupClient} view={view} workspace={WORKSPACE} />
  </ChatFixtureShell></ChatSummaryProvider></AppRouterContext>;
}

/** Match ShellFrame's grid and markers without introducing a Clerk session into dry tests. */
function ChatFixtureShell({children}: {children: ReactNode}) {
  return <div className="app-surface app-shell fixed inset-0 grid h-dvh grid-cols-1 grid-rows-[56px_1fr] bg-background text-foreground md:grid-cols-[auto_1fr]" data-surface="dashboard">
    <DashboardShellHeader className="px-0 md:px-0"><div className="flex w-56 items-center justify-between px-xl">
      <span className="flex items-center gap-sm"><BrandMark className="size-5" />agentsfleet</span><PanelLeftCloseIcon size={16} aria-hidden="true" />
    </div></DashboardShellHeader>
    <aside className="app-sidebar hidden min-h-0 w-56 flex-col bg-card md:flex"><nav aria-label="Primary" className="flex h-full flex-col gap-sm px-sm py-lg text-label">
      <a href={`/w/${WORKSPACE}/fleets/new`} className="flex items-center gap-md rounded-md px-md py-sm"><PlusIcon size={16} aria-hidden="true" />New fleet</a>
      <a href={`/w/${WORKSPACE}/fleets`} className="flex items-center gap-md rounded-md px-md py-sm"><PanelsTopLeftIcon size={16} aria-hidden="true" />Your fleets</a>
      <span className="mt-lg px-md text-eyebrow text-muted-foreground">Fleets</span>
      <a href={`/w/${WORKSPACE}/fleets/${FLEET_ID}`} aria-current="page" className="flex items-center gap-sm rounded-md bg-accent px-md py-sm"><span className="size-1.5 rounded-full bg-success" />ci-repairer</a>
      <span className="flex items-center gap-sm px-md py-sm"><span className="size-1.5 rounded-full bg-success" />ci-responder</span>
    </nav></aside>
    <main className="app-dashboard-canvas min-h-0 overflow-hidden"><div className="h-full min-h-0"><div data-fleet-workspace className="flex h-full min-h-0 flex-col">{children}</div></div></main>
  </div>;
}
