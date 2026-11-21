import type { TurnDetail } from "@/lib/api/turns";
import { useState } from "react";
import { AssistantRuntimeProvider, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";
import { Button, TooltipProvider } from "@agentsfleet/design-system";
import { renderFleetMessage } from "@/components/domain/activity/messages/fleetMessageRenderers";
import { FleetScopeProvider } from "@/components/domain/conversation/stream/FleetScope";
import { SenderLabelProvider } from "@/components/domain/activity/messages/FleetMessageRow";
import { ScheduleSync } from "@/components/domain/schedules/ScheduleSync";
import { useFleetThreadEntries } from "@/components/domain/conversation/thread/useFleetThreadEntries";
import { convertTurn } from "@/components/domain/conversation/stream/useFleetTurnStream";
import { rowToTurn } from "@/lib/streaming/fleet-stream-row";
import { FAILED_SCHEDULE, SCHEDULE } from "@/tests/helpers/draft-fixtures";
import { SCHEDULE_TURN, SCHEDULE_SNAPSHOT } from "@/tests/helpers/schedule-activity";

const RECEIPTS = [
  {kind: "schedule_changed", change: "created", sync: "syncing"},
  {kind: "schedule_sync_failed", sync: "failed", last_error: "Scheduler refused."},
  {kind: "schedule_registered", sync: "synced"},
  {kind: "schedule_changed", change: "paused", status: "paused", sync: "syncing"},
  {kind: "schedule_changed", change: "removal_requested", status: "deleting", sync: "syncing"},
  {kind: "schedule_removed", status: "removed", sync: "synced"},
];
const ROWS: TurnDetail[] = [
  ...RECEIPTS.map((receipt, index) => ({...SCHEDULE_TURN, turn_id: `schedule:${index}`, actor: "system", event_type: "activity",
    status: "recorded", request_json: JSON.stringify({...SCHEDULE_SNAPSHOT, ...receipt})})),
  {...SCHEDULE_TURN, response_text: "Support queue reviewed."},
  {...SCHEDULE_TURN, turn_id: "failed-fire", status: "fleet_error", failure_label: "oom_kill", failure_detail: "Memory cap reached"},
  {...SCHEDULE_TURN, turn_id: "queued-fire", status: "queued"},
];

/** Real grouping and chat renderers; recorded outcomes execute no scheduled work. */
export function ScheduleFixture() {
  const [rows, setRows] = useState(ROWS);
  const [retried, setRetried] = useState(false);
  const [delivered, setDelivered] = useState(false);
  const {entries, convertEntry} = useFleetThreadEntries(rows.map(rowToTurn), convertTurn);
  const runtime = useExternalStoreRuntime({messages: entries, convertMessage: convertEntry, onNew: async () => {}});
  const finish = () => {
    setRows(rows => rows.map(row => row.turn_id === "queued-fire" ? {...row, status: "processed", response_text: "Queued support review completed."} : row));
    setDelivered(true);
  };
  return <TooltipProvider><main className="min-h-dvh bg-background p-lg text-foreground">
    <h1>Support triage</h1>
    <ScheduleSync schedules={[{...(retried ? SCHEDULE : FAILED_SCHEDULE), last_error: "Scheduler refused."}]}
      busy={false} onRetry={() => setRetried(true)} />
    <Button variant="ghost" onClick={finish} disabled={delivered}>Deliver queued fire</Button>
    <FleetScopeProvider workspaceId={SCHEDULE_TURN.workspace_id} fleetId={SCHEDULE_TURN.fleet_id}>
      <SenderLabelProvider senderLabel="Support triage"><AssistantRuntimeProvider runtime={runtime}>
        <ThreadPrimitive.Root className="mx-auto w-full max-w-chat"><ThreadPrimitive.Messages>{renderFleetMessage}</ThreadPrimitive.Messages></ThreadPrimitive.Root>
      </AssistantRuntimeProvider></SenderLabelProvider>
    </FleetScopeProvider>
  </main></TooltipProvider>;
}
