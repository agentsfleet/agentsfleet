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
import { CONNECTION_STATUS, convertEvent } from "@/components/domain/conversation/stream/useFleetEventStream";
import { RENDER_KIND, RENDER_KIND_KEY, useFleetThreadEntries } from "@/components/domain/conversation/thread/useFleetThreadEntries";
import { FleetThreadViewport } from "@/components/domain/conversation/thread/FleetThreadViewport";
import { SenderNamesProvider } from "@/components/domain/shared/identity/SenderNames";
import { ACTOR } from "@/lib/events/event-summary";
import { senderNamesFrom } from "@/lib/events/sender-names";
import { rowToEvent } from "@/lib/streaming/fleet-stream-row";
import { SCHEDULE_EVENT } from "@/tests/helpers/schedule-activity";
import { FLEET_ID, WORKSPACE } from "@/tests/helpers/draft-fixtures";

const VIEWER = "fixture-owner";
const OPEN_FLEET = "support-triage";
const WATCH = "runner-link-watch";
const CHIEF_ID = "0199a801-1000-7000-8000-0000000000c0";
const STEER_TEXT = "Run the hourly runner link check and send me its first lines.";
const router = {bfcacheId: "address-fixture", back() {}, forward() {}, refresh() {}, push() {}, replace() {}, async prefetch() {}};
const ignore = (): void => undefined;
// The probe has no live stream for a send here to reconcile against.
const unreconciled = (): boolean => false;

/** The production composer and thread over the real `useMessageDelivery`; only the API is the spec's. */
export function ChatAddressFixture() {
  const [opened, setOpened] = useState("");
  const [painted, setPainted] = useState<ThreadMessageLike[]>([]);
  const ledger = useFleetPendingSends({subject: VIEWER, workspaceId: WORKSPACE, fleetId: FLEET_ID});
  const appendOptimistic = useCallback((text: string) => {
    const id = `optimistic-${crypto.randomUUID()}`;
    setPainted(rows => [...rows, {id, role: "user", content: text, metadata: {custom: {[RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, actor: ACTOR.PENDING_STEER}}}]);
    return id;
  }, []);
  const discardOptimistic = useCallback((id: string) => setPainted(rows => rows.filter(row => row.id !== id)), []);
  // Records where an addressed send landed, where the thread would push that fleet's page.
  const delivery = useMessageDelivery({workspaceId: WORKSPACE, fleetId: FLEET_ID, fleetName: OPEN_FLEET, openFleet: setOpened,
    appendOptimistic, reconcileOptimistic: unreconciled, discardOptimistic, writers: ledger.writers});
  const runtimeRef = useRef<AssistantRuntime | null>(null);
  const queue = useFleetSteerQueue(delivery.onNew, runtimeRef);
  const runtime = useExternalStoreRuntime({messages: painted, convertMessage: (row: ThreadMessageLike) => row, onNew: delivery.onNew, queue});
  useEffect(() => {
    runtimeRef.current = runtime;
    return () => { runtimeRef.current = null; };
  }, [runtime]);
  return <FixtureShell fleetName={OPEN_FLEET}>
    <output aria-label="Opened fleet" className="px-xl py-sm text-eyebrow text-muted-foreground">{opened}</output>
    <AssistantRuntimeProvider runtime={runtime}>
      <FleetThreadViewport modelLabel="MiniMax M3" eventsCount={painted.length} connectionStatus={CONNECTION_STATUS.LIVE} pending={ledger.pending}
        onRetry={ignore} onResend={delivery.resend} onDismiss={ledger.writers.dismiss} onRestored={delivery.noteRestored} onDraft={delivery.noteDraft} />
    </AssistantRuntimeProvider>
  </FixtureShell>;
}

/** One thread whose only turn another fleet steered, through the production row renderers. */
export function FleetSteerRowFixture() {
  const [events] = useState(() => [rowToEvent({...SCHEDULE_EVENT, workspace_id: WORKSPACE, fleet_id: FLEET_ID,
    turn_id: "0199a801-1000-7000-8000-0000000000e1", actor: `fleet:${CHIEF_ID}`, event_type: "chat", status: "queued",
    request_json: JSON.stringify({message: `[from fleet chief-of-staff, fleet_id ${CHIEF_ID}; answer with fleet_steer to that id]\n${STEER_TEXT}`,
      text: STEER_TEXT, sender_fleet_id: CHIEF_ID, sender_fleet_name: "chief-of-staff", sender_event_id: "0199a801-1000-7000-8000-0000000000e0"}),
    created_at: new Date().toISOString(), updated_at: new Date().toISOString()})]);
  const grouped = useFleetThreadEntries(events, convertEvent);
  const messages = useMemo(() => grouped.entries.map(grouped.convertEntry), [grouped]);
  const runtime = useExternalStoreRuntime({messages, convertMessage: (row: ThreadMessageLike) => row, onNew: async () => {}});
  return <FixtureShell fleetName={WATCH}>
    <AssistantRuntimeProvider runtime={runtime} config={FLEET_CHAT_CONFIG}>
      <FleetThreadViewport modelLabel="MiniMax M3" eventsCount={events.length} connectionStatus={CONNECTION_STATUS.LIVE} pending={[]}
        onRetry={ignore} onResend={ignore} onDismiss={ignore} onRestored={ignore} onDraft={ignore} />
    </AssistantRuntimeProvider>
  </FixtureShell>;
}

function FixtureShell({fleetName, children}: {fleetName: string; children: ReactNode}) {
  return <AppRouterContext value={router}><PathnameContext value={`/w/${WORKSPACE}/fleets/${FLEET_ID}`}><SearchParamsContext value={new URLSearchParams()}>
    <TooltipProvider><SenderLabelProvider senderLabel={fleetName}><SenderNamesProvider names={senderNamesFrom(VIEWER, [])}>
      <main className="flex h-dvh flex-col bg-background text-foreground">{children}</main>
    </SenderNamesProvider></SenderLabelProvider></TooltipProvider>
  </SearchParamsContext></PathnameContext></AppRouterContext>;
}
