import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { AssistantRuntimeProvider, useExternalStoreRuntime, type AssistantRuntime, type ThreadMessageLike } from "@assistant-ui/react";
import { AppRouterContext } from "next/dist/shared/lib/app-router-context.shared-runtime";
import { PathnameContext, SearchParamsContext } from "next/dist/shared/lib/hooks-client-context.shared-runtime";
import { TooltipProvider } from "@agentsfleet/design-system";
import { SenderLabelProvider } from "@/components/domain/activity/messages/FleetMessageRow";
import { FLEET_CHAT_CONFIG } from "@/components/domain/activity/tools/FleetToolRegistry";
import { useMessageDelivery } from "@/components/domain/conversation/delivery/useFleetMessageDelivery";
import { useFleetPendingSends } from "@/components/domain/conversation/delivery/useFleetPendingSends";
import { useFleetSteerQueue } from "@/components/domain/conversation/delivery/useFleetSteerQueue";
import { CONNECTION_STATUS, convertTurn } from "@/components/domain/conversation/stream/useFleetTurnStream";
import { RENDER_KIND, RENDER_KIND_KEY, useFleetThreadEntries } from "@/components/domain/conversation/thread/useFleetThreadEntries";
import { FleetThreadViewport } from "@/components/domain/conversation/thread/FleetThreadViewport";
import { SenderNamesProvider } from "@/components/domain/shared/identity/SenderNames";
import { ACTOR } from "@/lib/turns/turn-summary";
import { senderNamesFrom } from "@/lib/turns/sender-names";
import { rowToTurn, type FleetTurn } from "@/lib/streaming/fleet-stream-row";
import { readTurnDetailRoute } from "@/lib/streaming/fleet-stream-detail-reader";
import { isReplyInFlight } from "@/components/domain/activity/replies/fleetReplyMessage";
import { useTurnStop } from "@/components/domain/conversation/composer/useTurnStop";
import type { TurnRow } from "@/lib/api/turns";
import { STOP_TURNS } from "./fleet-stop-data";
import { SCHEDULE_TURN } from "@/tests/helpers/schedule-activity";
import { FLEET_ID, WORKSPACE } from "@/tests/helpers/fleet-fixtures";

const VIEWER = "fixture-owner";
const OPEN_FLEET = "support-triage";
const WATCH = "runner-link-watch";
const CHIEF = "chief-of-staff";
const WATCH_REPORT = "No action needed. The runner link stayed connected during this check.";
const CHIEF_ID = "0199a801-1000-7000-8000-0000000000c0";
const STEER_TEXT = "Run the hourly runner link check and send me its first lines.";
const router = {bfcacheId: "address-fixture", back() {}, forward() {}, refresh() {}, push() {}, replace() {}, async prefetch() {}};
const ignore = (): void => undefined;
// The probe has no live stream for a send here to reconcile against.
const unreconciled = (): boolean => false;

/** The production composer and thread over the real `useMessageDelivery`; only the API is the spec's. */
type FleetLocation = {workspaceId?: string; fleetId?: string; fleetName?: string};

export function ChatAddressFixture({workspaceId = WORKSPACE, fleetId = FLEET_ID, fleetName = OPEN_FLEET}: FleetLocation) {
  const [opened, setOpened] = useState("");
  const [painted, setPainted] = useState<ThreadMessageLike[]>([]);
  const ledger = useFleetPendingSends({subject: VIEWER, workspaceId, fleetId});
  const appendOptimistic = useCallback((text: string) => {
    const id = `optimistic-${crypto.randomUUID()}`;
    setPainted(rows => [...rows, {id, role: "user", content: text, metadata: {custom: {[RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, actor: ACTOR.PENDING_STEER}}}]);
    return id;
  }, []);
  const discardOptimistic = useCallback((id: string) => setPainted(rows => rows.filter(row => row.id !== id)), []);
  // Records where an addressed send landed, where the thread would push that fleet's page.
  const delivery = useMessageDelivery({workspaceId, fleetId, fleetName, openFleet: setOpened,
    appendOptimistic, reconcileOptimistic: unreconciled, discardOptimistic, writers: ledger.writers});
  const runtimeRef = useRef<AssistantRuntime | null>(null);
  const queue = useFleetSteerQueue(delivery.onNew, runtimeRef);
  const runtime = useExternalStoreRuntime({messages: painted, convertMessage: (row: ThreadMessageLike) => row, onNew: delivery.onNew, queue});
  useEffect(() => {
    runtimeRef.current = runtime;
    return () => { runtimeRef.current = null; };
  }, [runtime]);
  return <FixtureShell workspaceId={workspaceId} fleetId={fleetId} fleetName={fleetName}>
    <output aria-label="Opened fleet" className="px-xl py-sm text-eyebrow text-muted-foreground">{opened}</output>
    <AssistantRuntimeProvider runtime={runtime}>
      <FleetThreadViewport modelLabel="MiniMax M3" turnsCount={painted.length} connectionStatus={CONNECTION_STATUS.LIVE} pending={ledger.pending}
        onRetry={ignore} onResend={delivery.resend} onDismiss={ledger.writers.dismiss} onRestored={delivery.noteRestored} onDraft={delivery.noteDraft} />
    </AssistantRuntimeProvider>
  </FixtureShell>;
}

/** Recorded-shape examples of the dispatch and the report, rendered through the production thread. */
export function FleetSteerRowFixture({report}: {report: boolean}) {
  const recipient = report ? CHIEF : WATCH;
  const recipientId = report ? CHIEF_ID : FLEET_ID;
  const senderId = report ? FLEET_ID : CHIEF_ID;
  const sender = report ? WATCH : CHIEF;
  const words = report ? WATCH_REPORT : STEER_TEXT;
  const [turns] = useState(() => {
    const handoff = rowToTurn({...SCHEDULE_TURN, fleet_id: recipientId,
      turn_id: "0199a801-1000-7000-8000-0000000000e1", actor: `fleet:${senderId}`, event_type: "chat",
      status: report ? "processed" : "queued", created_at: "2026-11-04T03:32:00Z", updated_at: "2026-11-04T03:33:00Z",
      request_json: JSON.stringify({message: `[from fleet ${sender}, fleet_id ${senderId}; answer with fleet_steer to that id]\n${words}`,
        sender_fleet_id: senderId, sender_fleet_name: sender, sender_event_id: "0199a801-1000-7000-8000-0000000000e0"}),
      ...(report ? {wall_ms: 2000, tool_calls: {calls: [{call_id: "2:1", name: "fleet_activity",
        status: "succeeded", arguments: {fleet_id: senderId}, duration_ms: 53, output_head: "Check completed", output_line_count: 1}]}} : {}),
      response_text: report ? "Runner Link Watch found no interruptions. No action is needed." : null});
    if (!report) return [handoff];
    const request = rowToTurn({...SCHEDULE_TURN, fleet_id: CHIEF_ID,
      turn_id: "0199a801-1000-7000-8000-0000000000cf", actor: `steer:${VIEWER}`, event_type: "chat",
      request_json: JSON.stringify({message: "Have runner-link-watch run its check and tell me the result."}),
      wall_ms: 3000, tool_calls: {calls: [{call_id: "1:1", name: "fleet_steer", status: "succeeded",
        arguments: {fleet_id: FLEET_ID, text: STEER_TEXT}, duration_ms: 70, output_head: "Request accepted", output_line_count: 1}]},
      response_text: "I asked @runner-link-watch to check the connection. It will report back here."});
    return [request, handoff];
  });
  return <RecordedFleetThread turns={turns} fleetId={recipientId} fleetName={recipient} />;
}

export function RecordedFleetThread({turns, fleetId, fleetName, workspaceId = WORKSPACE}:
  {turns: ReturnType<typeof rowToTurn>[]; fleetId: string; fleetName: string; workspaceId?: string}) {
  const grouped = useFleetThreadEntries(turns, convertTurn);
  const messages = useMemo(() => grouped.entries.map(grouped.convertEntry), [grouped]);
  const runtime = useExternalStoreRuntime({messages, convertMessage: (row: ThreadMessageLike) => row, onNew: async () => {}});
  return <FixtureShell fleetName={fleetName} fleetId={fleetId} workspaceId={workspaceId}>
    <AssistantRuntimeProvider runtime={runtime} config={FLEET_CHAT_CONFIG}>
      <FleetThreadViewport modelLabel="MiniMax M3" turnsCount={turns.length} connectionStatus={CONNECTION_STATUS.LIVE} pending={[]}
        onRetry={ignore} onResend={ignore} onDismiss={ignore} onRestored={ignore} onDraft={ignore} />
    </AssistantRuntimeProvider>
  </FixtureShell>;
}

const STOP_READ_MS = 200;

/** Two of a person's turns, one running and one queued behind it, under the
 * production thread, composer and Stop. The probe has no stream: it reads each
 * unsettled turn's saved row from the same-origin route the chat reads when a
 * reply's ending never reached it, so the spec decides when an answer lands. */
export function FleetStopFixture() {
  const [turns, setTurns] = useState<FleetTurn[]>(() => [
    stopRow(STOP_TURNS.running, "received", "Rotate the staging deploy key."),
    stopRow(STOP_TURNS.queued, "queued", "Then post the new key's fingerprint."),
  ].map(rowToTurn));
  const reconcile = useCallback((rows: TurnRow[]) => setTurns(current => current.map(turn => {
    const saved = rows.find(row => row.turn_id === turn.id);
    return saved === undefined ? turn : rowToTurn(saved);
  })), []);
  useEffect(() => {
    const timer = setInterval(() => {
      for (const turn of turns.filter(isReplyInFlight)) {
        void readTurnDetailRoute(WORKSPACE, FLEET_ID, turn.id).then(read => { if (read.ok) reconcile([read.data]); });
      }
    }, STOP_READ_MS);
    return () => clearInterval(timer);
  }, [turns, reconcile]);
  const turnStop = useTurnStop(WORKSPACE, FLEET_ID, turns, reconcile);
  const grouped = useFleetThreadEntries(turns, convertTurn);
  const messages = useMemo(() => grouped.entries.map(grouped.convertEntry), [grouped]);
  const runtime = useExternalStoreRuntime({messages, convertMessage: (row: ThreadMessageLike) => row, onNew: async () => {}});
  return <FixtureShell fleetName={OPEN_FLEET}>
    <AssistantRuntimeProvider runtime={runtime} config={FLEET_CHAT_CONFIG}>
      <FleetThreadViewport modelLabel="MiniMax M3" turnsCount={turns.length} connectionStatus={CONNECTION_STATUS.LIVE} pending={[]}
        onRetry={ignore} onResend={ignore} onDismiss={ignore} onRestored={ignore} onDraft={ignore} turnStop={turnStop} />
    </AssistantRuntimeProvider>
  </FixtureShell>;
}

function stopRow(turn_id: string, status: string, message: string) {
  return {...SCHEDULE_TURN, workspace_id: WORKSPACE, fleet_id: FLEET_ID, turn_id, actor: `${ACTOR.STEER_PREFIX}${VIEWER}`,
    event_type: "chat", status, request_json: JSON.stringify({message}), created_at: new Date().toISOString(), updated_at: new Date().toISOString()};
}

function FixtureShell({fleetName, fleetId = FLEET_ID, workspaceId = WORKSPACE, children}: FleetLocation & {fleetName: string; children: ReactNode}) {
  return <AppRouterContext value={router}><PathnameContext value={`/w/${workspaceId}/fleets/${fleetId}`}><SearchParamsContext value={new URLSearchParams()}>
    <TooltipProvider><SenderLabelProvider senderLabel={fleetName}><SenderNamesProvider names={senderNamesFrom(VIEWER, [])}>
      <main className="flex h-dvh flex-col bg-background text-foreground">{children}</main>
    </SenderNamesProvider></SenderLabelProvider></TooltipProvider>
  </SearchParamsContext></PathnameContext></AppRouterContext>;
}
