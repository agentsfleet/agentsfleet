import { useState, type ReactNode } from "react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { ShellFrame } from "@/components/layout/ShellFrame";
import { OWN_ACCOUNT } from "@/tests/helpers/workspace-fixtures";
import { PathnameContext, SearchParamsContext } from "next/dist/shared/lib/hooks-client-context.shared-runtime";
import { SenderNamesProvider } from "@/components/domain/shared/identity/SenderNames";
import { SenderLabelProvider } from "@/components/domain/activity/messages/FleetMessageRow";
import { senderNamesFrom } from "@/lib/turns/sender-names";
import { AssistantRuntimeProvider, useExternalStoreRuntime, type ThreadMessageLike } from "@assistant-ui/react";
import { AppRouterContext } from "next/dist/shared/lib/app-router-context.shared-runtime";
import { FleetHeader } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetHeader";
import { FleetConversationLayout } from "@/components/domain/shared/summary/FleetWorkSidebar";
import { FleetThreadViewport } from "@/components/domain/conversation/thread/FleetThreadViewport";
import { DurableTaskPanel } from "@/components/domain/tasks/TaskPanel";
import { CONNECTION_STATUS } from "@/components/domain/conversation/stream/useFleetTurnStream";
import { RENDER_KIND, RENDER_KIND_KEY } from "@/components/domain/conversation/thread/useFleetThreadEntries";
import { ACTOR } from "@/lib/turns/turn-summary";
import { CONTEXT_STATE } from "@/lib/context-progress";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { FLEET_DETAIL, WORKSPACE, FLEET_ID } from "@/tests/helpers/fleet-fixtures";

const router = {bfcacheId: "visual-fixture", back() {}, forward() {}, refresh() {}, push() {}, replace() {}, async prefetch() {}};
const exchanges = [
  ["What is happening with the Linkwarden repair?", "The selected run failed its callback regression. I’m inspecting the recorded failure before changing the source."],
  ["Use that run and attempt. Keep the existing assertions.", "I’ll keep the repair within the reviewed repository and preserve the regression assertions."],
  ["Require passing checks before marking the repair done.", "The completion criterion will require passing checks on the proposed revision. Jev evaluates the recorded evidence against that criterion."],
  ["Remember to link the evidence in every report.", "I pinned that, so every later run starts with it. You can remove it under Remembers."],
  ["Show me the durable task and its progress.", "Summary shows the saved objective, criteria and recorded progress. You can keep chatting while the fleet works."],
];
const history: ThreadMessageLike[] = exchanges.flatMap(([question, answer], index) => [
  {id: `operator-${index}`, role: "user", content: question ?? "", metadata: {custom: {[RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, actor: `${ACTOR.STEER_PREFIX}fixture-owner`}}},
  {id: `reply-${index}`, role: "assistant", content: answer ?? ""},
]);

/** Production header, transcript, composer and task reader; only API data is simulated. */
export function ChatAlignmentFixture() {
  const query = new URLSearchParams(location.search);
  const [messages, setMessages] = useState(() => query.has("scale")
    ? Array.from({length: 5}, (_, batch) => history.map(message => ({...message, id: `${batch}-${message.id}`}))).flat()
    : history);
  const [connectionStatus, setConnectionStatus] = useState(() => query.has("offline") ? CONNECTION_STATUS.OFFLINE : CONNECTION_STATUS.LIVE);
  const runtime = useExternalStoreRuntime({messages, convertMessage: (message: ThreadMessageLike) => message,
    onNew: async message => {setMessages(previous => [...previous, {id: `sent-${previous.length}`, role: "user", content: message.content,
      metadata: {custom: {[RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, actor: `${ACTOR.STEER_PREFIX}fixture-owner`}}}]);}});
  return <AppRouterContext value={router}><ChatFixtureShell>
    <FleetHeader workspaceId={WORKSPACE} fleet={FleetDetailSchema.parse({...FLEET_DETAIL, name: "ci-repairer", status: "active"})} />
    <AssistantRuntimeProvider runtime={runtime}><FleetConversationLayout work={<DurableTaskPanel workspaceId={WORKSPACE} fleetId={FLEET_ID} subject="fixture-owner" revision={0} sources={{repositories: ["agentsfleet/linkwarden"], access: "write", publication: "existing_pr"}} />}>
      <FleetThreadViewport modelLabel="MiniMax M3.1 Flash" turnsCount={messages.length} connectionStatus={connectionStatus}
        pending={[]} onRetry={() => setConnectionStatus(CONNECTION_STATUS.LIVE)} onResend={() => {}} onDismiss={() => {}} onRestored={() => {}} onDraft={() => {}} />
    </FleetConversationLayout></AssistantRuntimeProvider>
  </ChatFixtureShell></AppRouterContext>;
}


const OWNER = {[RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, actor: `${ACTOR.STEER_PREFIX}fixture-owner`};
const SAMPLE_AT = new Date();
const SAMPLE: ThreadMessageLike[] = [
  {id: "sample-request", role: "user", createdAt: SAMPLE_AT, content: "Investigate the failing test and explain the evidence.", metadata: {custom: OWNER}},
  {id: "sample-answer", role: "assistant", createdAt: SAMPLE_AT, content: "The request has an owner. Switching conversations cancels its stream.\n\n- Stop closes the active reader.\n- Retry preserves the request identity.\n- The evidence stays with this conversation."},
];

/** A short fleet conversation beside its Summary: production thread, composer and task reader over sample turns. */
export function FleetSampleFixture() {
  const [messages] = useState(SAMPLE);
  const runtime = useExternalStoreRuntime({messages, convertMessage: (message: ThreadMessageLike) => message, onNew: async () => {}});
  return <AppRouterContext value={router}><PathnameContext value={`/w/${WORKSPACE}/fleets/${FLEET_ID}`}><SearchParamsContext value={new URLSearchParams()}>
    <TooltipProvider><SenderLabelProvider senderLabel="ci-repairer"><SenderNamesProvider names={senderNamesFrom("fixture-owner", [])}>
      <main className="flex h-dvh flex-col bg-background text-foreground">
        <p className="px-xl py-sm text-eyebrow text-muted-foreground">Sample data · Chat component review</p>
        <AssistantRuntimeProvider runtime={runtime}><FleetConversationLayout work={<DurableTaskPanel workspaceId={WORKSPACE} fleetId={FLEET_ID} subject="fixture-owner" revision={0} sources={{repositories: ["linkwarden/linkwarden"], access: "write", publication: "existing_pr"}} />}>
          <FleetThreadViewport modelLabel="MiniMax M3.1 Flash" turnsCount={messages.length} connectionStatus={CONNECTION_STATUS.LIVE}
            pending={[]} onRetry={() => {}} onResend={() => {}} onDismiss={() => {}} onRestored={() => {}} onDraft={() => {}} />
        </FleetConversationLayout></AssistantRuntimeProvider>
      </main>
    </SenderNamesProvider></SenderLabelProvider></TooltipProvider>
  </SearchParamsContext></PathnameContext></AppRouterContext>;
}

/** A fleet's reply before its answer: queued, working, compacting its context, or thinking. */
export function LoadingReplyFixture({state}: {state: string}) {
  const reply: ThreadMessageLike = {id: "loading-reply", role: "assistant", createdAt: SAMPLE_AT, status: {type: "running"},
    content: state === "thinking" ? [{type: "reasoning", text: "Reading the failing test before changing the source."}] : [],
    metadata: {custom: {queued: state === "queued",
      ...(state === "compacting" ? {contextProgress: {generation: 1, state: CONTEXT_STATE.summarizing}} : {})}}};
  const [messages] = useState<ThreadMessageLike[]>(() => [SAMPLE[0] ?? reply, reply]);
  const runtime = useExternalStoreRuntime({messages, isRunning: true, convertMessage: (message: ThreadMessageLike) => message, onNew: async () => {}});
  return <SenderLabelProvider senderLabel="ci-repairer"><SenderNamesProvider names={senderNamesFrom("fixture-owner", [])}>
    <main className="flex h-dvh flex-col bg-background text-foreground">
      <AssistantRuntimeProvider runtime={runtime}>
        <FleetThreadViewport modelLabel="MiniMax M3.1 Flash" turnsCount={messages.length} connectionStatus={CONNECTION_STATUS.LIVE}
          pending={[]} onRetry={() => {}} onResend={() => {}} onDismiss={() => {}} onRestored={() => {}} onDraft={() => {}} />
      </AssistantRuntimeProvider>
    </main>
  </SenderNamesProvider></SenderLabelProvider>;
}

/** Render the production shell; only identity and API data are supplied by fixtures. */
function ChatFixtureShell({children}: {children: ReactNode}) {
  const path = new URLSearchParams(location.search).has("secrets") ? "secrets" : `fleets/${FLEET_ID}`;
  return <PathnameContext value={`/w/${WORKSPACE}/${path}`}><SearchParamsContext value={new URLSearchParams()}>
    <TooltipProvider><ShellFrame workspaces={[{id: WORKSPACE, name: "Release workspace", created_at: "2026-01-01T00:00:00Z", ...OWN_ACCOUNT}]}>
      <SenderLabelProvider senderLabel="ci-repairer"><SenderNamesProvider names={senderNamesFrom("fixture-owner", [])}>
        <div data-fleet-workspace className="flex h-full min-h-0 flex-col">{children}</div>
      </SenderNamesProvider></SenderLabelProvider>
    </ShellFrame></TooltipProvider>
  </SearchParamsContext></PathnameContext>;
}
