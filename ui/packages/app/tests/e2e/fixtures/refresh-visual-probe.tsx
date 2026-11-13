import {visualWakeup} from "./refresh-wakeup-data";
import {FleetScopeProvider} from "@/components/domain/FleetScope";
import { useCallback, useMemo, useState, type ReactNode } from "react";
import { AssistantRuntimeProvider, useExternalStoreRuntime, type AppendMessage, type ThreadMessageLike } from "@assistant-ui/react";
import { AppRouterContext } from "next/dist/shared/lib/app-router-context.shared-runtime";
import { Button, TooltipProvider } from "@agentsfleet/design-system";
import { ShellFrame } from "@/components/layout/ShellFrame";
import { PathnameContext, SearchParamsContext } from "next/dist/shared/lib/hooks-client-context.shared-runtime";
import { OWN_ACCOUNT } from "@/tests/helpers/workspace-fixtures";
import { FleetHeader } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetHeader";
import { FleetConversationLayout } from "@/components/domain/FleetWorkSidebar";
import { FleetThreadViewport } from "@/components/domain/FleetThreadViewport";
import { SenderLabelProvider } from "@/components/domain/FleetMessageRow";
import { SenderNamesProvider } from "@/components/domain/SenderNames";
import { senderNamesFrom } from "@/lib/events/sender-names";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { toReplyMessage, replyParts } from "@/components/domain/fleetReplyMessage";
import { VISUAL_SCENARIOS, visualEvents, type VisualScenario } from "./refresh-visual-data";
import { TaskPanel } from "@/components/domain/chat/TaskPanel";
import { FleetTaskProvider } from "@/components/domain/chat/FleetTaskProvider";
import { CONNECTION_STATUS, convertEvent } from "@/components/domain/useFleetEventStream";
import { RENDER_KIND, RENDER_KIND_KEY, useFleetThreadEntries } from "@/components/domain/useFleetThreadEntries";
import { ACTOR } from "@/lib/events/event-summary";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { FLEET_DETAIL, WORKSPACE, FLEET_ID } from "@/tests/helpers/draft-fixtures";
import { useFleetUploadPreviews } from "@/components/domain/useFleetUploadPreviews";
import { FLEET_CHAT_CONFIG } from "@/components/domain/FleetToolRegistry";
import { FleetSetupWorkspace } from "@/components/domain/chat/FleetSetupWorkspace";
import { chatMessageText } from "@/lib/chat-message-text";
import { latestFigures } from "@/lib/events/run-summary";
import {VisualLifecycleNotice} from "./visual-lifecycle-notice";

const router = { bfcacheId: "visual-review", back() {}, forward() {}, refresh() {}, push() {}, replace() {}, async prefetch() {} };


/** Current production conversation, activity, composer, and Summary; sample data. */
export function RefreshVisualFixture() {
  const archived = new URLSearchParams(location.search).has("archived");
  const [started] = useState(Date.now);
  const [phase, setPhase] = useState<VisualScenario>(() => VISUAL_SCENARIOS.find(value => value === new URLSearchParams(location.search).get("state")) ?? "running");
  const showScenario = useCallback((value: VisualScenario) => {const url = new URL(location.href); url.searchParams.set("state", value); history.replaceState(null, "", url); setPhase(value);}, []);
  const events = useMemo(() => visualEvents(phase, started, new URLSearchParams(location.search).has("code")), [phase, started]);
  const grouped = useFleetThreadEntries(events, convertEvent);
  const fleetName = phase === "wake-up" ? "Merge watch" : "Release watch";
  const [sent, setSent] = useState<ThreadMessageLike[]>([]);
  const onNew = useCallback(async (message: AppendMessage) => {
    if (phase === "error") showScenario("running");
    setSent(previous => [...previous, {id: `local-${previous.length}`, role: "user", content: [{type: "text", text: chatMessageText(message)}]}]);
  }, [phase, showScenario]);
  const previews = useFleetUploadPreviews({workspaceId: WORKSPACE, fleetId: FLEET_ID, subject: "fixture-owner", events, onNew});
  const messages = useMemo(() => [...(phase === "wake-up" ? grouped.entries.map(grouped.convertEntry) : conversation(events, started)), ...sent, ...previews.entries.map(entry => entry.message)], [events, started, sent, previews.entries, phase, grouped.entries, grouped.convertEntry]);
  const runtime = useExternalStoreRuntime({ messages, isDisabled: archived, isRunning: !archived && phase === "running",
    convertMessage: (message: ThreadMessageLike) => message,
    onNew: previews.onNew, adapters: {attachments: previews.attachments},
    onCancel: async () => { showScenario("completed"); },
  });
  return <AppRouterContext value={router}><PathnameContext.Provider value={`/w/${WORKSPACE}/fleets/${FLEET_ID}`}><SearchParamsContext.Provider value={new URLSearchParams()}><TooltipProvider><ReviewChrome>
    <div className="flex min-h-0 flex-1 flex-col">
      <div data-visual-controls="" className="fixed inset-x-0 top-0 z-50 flex h-11.5 items-center gap-sm overflow-x-auto whitespace-nowrap bg-surface-deep px-lg [&>button]:shrink-0 [&>span]:shrink-0">
        <span className="mr-auto text-eyebrow text-muted-foreground">Sample data · Design comparison</span>
        {VISUAL_SCENARIOS.map(value => <Button key={value} size="sm" variant={phase === value ? "secondary" : "ghost"} aria-pressed={phase === value} onClick={() => showScenario(value)}>{value === "empty" ? "Empty" : value === "running" ? "Working" : value === "completed" ? "Completed" : value === "activity" ? "Activity" : value === "wake-up" ? "Wake-up" : "Error"}</Button>)}
        <Button size="sm" variant="ghost" onClick={() => { document.documentElement.dataset.theme = document.documentElement.dataset.theme === "light" ? "dark" : "light"; }}>Theme</Button>
      </div>
      <FleetSetupWorkspace account={OWN_ACCOUNT.account.tenant_id} workspace={WORKSPACE} fleet={FLEET_ID} revision={FLEET_DETAIL.setup_revision} viewer="fixture-owner" header={null}>
      {archived ? <VisualLifecycleNotice outcome="archived" /> : null}
      <FleetHeader latestRun={latestFigures(events)} workspaceId={WORKSPACE} fleet={FleetDetailSchema.parse({ ...FLEET_DETAIL, name: fleetName, status: archived ? "killed" : "active", schedules: phase === "wake-up" ? [visualWakeup(started)] : [], pending_approvals: !archived && phase === "completed" ? 1 : 0 })} />
      <AssistantRuntimeProvider runtime={runtime} config={FLEET_CHAT_CONFIG}><FleetScopeProvider workspaceId={WORKSPACE} fleetId={FLEET_ID}><SenderLabelProvider senderLabel={fleetName}><SenderNamesProvider names={senderNamesFrom("fixture-owner", [])}>
        <FleetTaskProvider readOnly={archived} workspaceId={WORKSPACE} fleetId={FLEET_ID} subject="fixture-owner" revision={VISUAL_SCENARIOS.indexOf(phase)} events={events} schedules={phase === "wake-up" ? [visualWakeup(started)] : []} sources={{repositories: phase === "empty" ? [] : ["agentsfleet/platform-images"], access: "write", publication: "existing_pr"}}
          approvals={{count: phase === "completed" ? 1 : 0, href: `/w/${WORKSPACE}/fleets/${FLEET_ID}#fleet-approvals`}}>
        <FleetConversationLayout work={<TaskPanel workspaceId={WORKSPACE} fleetId={FLEET_ID} subject="fixture-owner" revision={VISUAL_SCENARIOS.indexOf(phase)} />}>
          <div className="flex min-h-0 flex-1 flex-col">
            <FleetThreadViewport archived={archived} modelLabel="MiniMax M3" eventsCount={messages.length} connectionStatus={CONNECTION_STATUS.LIVE}
              pending={[]} onRetry={() => {}} onResend={() => {}} onDismiss={() => {}} onRestored={() => {}} onDraft={() => {}} />
          </div>
        </FleetConversationLayout>
        </FleetTaskProvider>
      </SenderNamesProvider></SenderLabelProvider></FleetScopeProvider></AssistantRuntimeProvider></FleetSetupWorkspace>
    </div>
  </ReviewChrome></TooltipProvider></SearchParamsContext.Provider></PathnameContext.Provider></AppRouterContext>;
}

function ReviewChrome({ children }: { children: ReactNode }) {
  return <><style>{".app-shell{top:46px;height:calc(100dvh - 46px)}"}</style>
    <ShellFrame workspaces={[{id: WORKSPACE, name: "Release workspace", created_at: "2026-01-01T00:00:00Z", ...OWN_ACCOUNT}]}>
      <div data-fleet-workspace="" className="flex h-full min-h-0 flex-col">{children}</div>
    </ShellFrame>
  </>;
}

function conversation(events: FleetEvent[], started: number): ThreadMessageLike[] {
  const event = events[0];
  if (!event) return [];
  const actor = `${ACTOR.STEER_PREFIX}fixture-owner`;
  const answer = toReplyMessage(convertEvent(event), event, actor);
  const [intro, ...rest] = event.reply.split("\n\n");
  return [
    {id: "request", role: "user", createdAt: new Date(started - 138_000),
      content: "Update the runner image using the release checklist. Run the scans, then ask me before publishing.",
      attachments: [{id: "sample-release-notes", type: "file", name: "release-notes.md", contentType: "text/markdown", status: {type: "complete"}, content: []}],
      metadata: {custom: {[RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, actor}}},
    {...answer, content: [{type: "text", text: intro ?? ""}, ...replyParts(event).filter(part => part.type !== "text"),
      ...(rest.length ? [{type: "text" as const, text: rest.join("\n\n")}] : [])]},
  ];
}
