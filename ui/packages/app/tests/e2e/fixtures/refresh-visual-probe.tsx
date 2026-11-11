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
import { STATUS_AGENT_ERROR } from "@/components/domain/fleetMessageStatus";
import { TaskPanel } from "@/components/domain/chat/TaskPanel";
import { CONNECTION_STATUS } from "@/components/domain/useFleetEventStream";
import { RENDER_KIND, RENDER_KIND_KEY } from "@/components/domain/useFleetThreadEntries";
import { ACTOR } from "@/lib/events/event-summary";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { FLEET_DETAIL, WORKSPACE, FLEET_ID } from "@/tests/helpers/draft-fixtures";
import { useFleetUploadPreviews } from "@/components/domain/useFleetUploadPreviews";
import { FLEET_CHAT_CONFIG } from "@/components/domain/FleetToolRegistry";
import { useFleetNotice } from "@/components/layout/FleetNoticeProvider";

const router = { bfcacheId: "visual-review", back() {}, forward() {}, refresh() {}, push() {}, replace() {}, async prefetch() {} };
type Scenario = "running" | "completed" | "error";

/** Current production conversation, activity, composer, and Summary; sample data. */
export function RefreshVisualFixture() {
  const [started] = useState(Date.now);
  const [phase, setPhase] = useState<Scenario>("running");
  const [sent, setSent] = useState<ThreadMessageLike[]>([]);
  const onNew = useCallback(async (message: AppendMessage) => {
    if (phase === "error") setPhase("running");
    setSent(previous => [...previous, {id: `local-${previous.length}`, role: "user", content: message.content}]);
  }, [phase]);
  const previews = useFleetUploadPreviews({workspaceId: WORKSPACE, fleetId: FLEET_ID, subject: "fixture-owner", events: [], onNew});
  const messages = useMemo(() => [...conversation(phase, started), ...sent, ...previews.entries.map(entry => entry.message)], [phase, started, sent, previews.entries]);
  const runtime = useExternalStoreRuntime({ messages, isRunning: phase === "running",
    convertMessage: (message: ThreadMessageLike) => message,
    onNew: previews.onNew, adapters: {attachments: previews.attachments},
    onCancel: async () => { setPhase("completed"); },
  });
  return <AppRouterContext value={router}><PathnameContext.Provider value={`/w/${WORKSPACE}/fleets/${FLEET_ID}`}><SearchParamsContext.Provider value={new URLSearchParams()}><TooltipProvider><ReviewChrome>
    <div className="flex min-h-0 flex-1 flex-col">
      <div data-visual-controls="" className="fixed inset-x-0 top-0 z-50 flex h-11 items-center gap-sm overflow-x-auto whitespace-nowrap bg-surface-deep px-lg [&>button]:shrink-0 [&>span]:shrink-0">
        <span className="mr-auto text-eyebrow text-muted-foreground">Implementation review · sample activity</span>
        {(["running", "completed", "error"] as const).map(value => <Button key={value} size="sm" variant={phase === value ? "secondary" : "ghost"} aria-pressed={phase === value} onClick={() => setPhase(value)}>{value === "running" ? "Working" : value === "completed" ? "Completed" : "Error"}</Button>)}
        <Button size="sm" variant="ghost" onClick={() => { document.documentElement.dataset.theme = document.documentElement.dataset.theme === "light" ? "dark" : "light"; }}>Theme</Button>
        <NoticeExamples />
      </div>
      <FleetHeader workspaceId={WORKSPACE} fleet={FleetDetailSchema.parse({ ...FLEET_DETAIL, name: "Release watch", status: "active" })} />
      <AssistantRuntimeProvider runtime={runtime} config={FLEET_CHAT_CONFIG}><SenderLabelProvider senderLabel="Release watch"><SenderNamesProvider names={senderNamesFrom("fixture-owner", [])}>
        <FleetConversationLayout work={<TaskPanel workspaceId={WORKSPACE} fleetId={FLEET_ID} subject="fixture-owner" revision={0} sources={{ repositories: ["agentsfleet/linkwarden"], access: "write", publication: "existing_pr" }}
          approvals={{count: 1, href: `/w/${WORKSPACE}/approvals?fleetId=${FLEET_ID}`}}
          schedules={[{schedule_id: "release-check", fleet_id: FLEET_ID, sync: "synced", status: "active", cron: "*/30 * * * *",
            timezone: "UTC", message: "Check release readiness", description: "Check release readiness", run_state: "scheduled", next_run_at: new Date(started + 30 * 60_000).toISOString()}]} />}>
          <div className="flex min-h-0 flex-1 flex-col">
            <FleetThreadViewport modelLabel="MiniMax M3" eventsCount={messages.length} connectionStatus={CONNECTION_STATUS.LIVE}
              pending={[]} onRetry={() => {}} onResend={() => {}} onDismiss={() => {}} onRestored={() => {}} onDraft={() => {}} />
          </div>
        </FleetConversationLayout>
      </SenderNamesProvider></SenderLabelProvider></AssistantRuntimeProvider>
    </div>
  </ReviewChrome></TooltipProvider></SearchParamsContext.Provider></PathnameContext.Provider></AppRouterContext>;
}

function NoticeExamples() {
  const show = useFleetNotice();
  return <>{(["archived", "deleted"] as const).map(outcome => <Button key={outcome} size="sm" variant="ghost"
    onClick={() => show({outcome, historyHref: `/w/${WORKSPACE}/fleets/${FLEET_ID}`})}>Sample {outcome} notice</Button>)}</>;
}

function ReviewChrome({ children }: { children: ReactNode }) {
  return <><style>{".app-shell{top:44px;height:calc(100dvh - 44px)}"}</style>
    <ShellFrame workspaces={[{id: WORKSPACE, name: "Release workspace", created_at: "2026-01-01T00:00:00Z", ...OWN_ACCOUNT}]}>
      <div data-fleet-workspace="" className="flex h-full min-h-0 flex-col">{children}</div>
    </ShellFrame>
  </>;
}

function conversation(phase: Scenario, started: number): ThreadMessageLike[] {
  const call = (id: string, toolName: string, args: Record<string, string>, running = false) => ({
    type: "tool-call" as const, toolCallId: id, toolName, args, argsText: JSON.stringify(args),
    timing: { startedAt: started - 12000, ...(running ? {} : { completedAt: started - 10000 }) },
    ...(running ? {} : { result: { status: phase === "error" && id === "scan" ? "failed" : "succeeded", outputHead: "Recorded example result", outputLineCount: 1 } }),
  });
  return [
    { id: "request", role: "user", content: "Watch this release. Check both image builds and the security scans, then tell me when it’s ready to merge.", metadata: { custom: { [RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, actor: `${ACTOR.STEER_PREFIX}fixture-owner` } } },
    { id: "answer", role: "assistant", metadata: {custom: phase === "error" ? {
      status: STATUS_AGENT_ERROR, requestActor: `${ACTOR.STEER_PREFIX}fixture-owner`,
      requestJson: JSON.stringify({message: "Check release readiness"}),
      failureLabel: "Provider connection interrupted", failureDetail: "Earlier build results are retained.",
    } : {}}, content: [
      { type: "text", text: "Both image builds passed for amd64 and arm64. I’m checking the final security scan before marking the release ready." },
      call("read", "http_request", { url: "https://api.github.com/repos/agentsfleet/linkwarden/actions/runs" }),
      call("scan", "exec_command", { cmd: "trivy image --severity CRITICAL release:latest" }, phase === "running"),
      ...(phase === "completed" ? [
        call("file", "file_read", {path: "release.yaml"}),
        call("edit", "file_edit", {path: "release.yaml", old_text: "ready: false", new_text: "ready: true"}),
        call("report", "custom_release_report", {release: "current"}),
        {type: "text" as const, text: "The security scan passed. Both image builds and the required checks are green. The release is ready for your review.\n\nReview `release.yaml` and [the build report](https://example.test/release).\n\n```yaml\nready: true\n```"},
      ] : []),
    ], status: phase === "running" ? { type: "running" } : { type: "complete", reason: "stop" } },
  ];
}
