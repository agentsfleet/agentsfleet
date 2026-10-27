"use client";

import { useEffect, useMemo, useRef, type ReactNode } from "react";
import { ACTIVITY_KIND, FLEET_ACTIVITY, readFleetActivity } from "@/lib/events/fleet-activity";
import {
  AssistantRuntimeProvider,
  useExternalStoreRuntime,
  type AssistantRuntime,
} from "@assistant-ui/react";
import {
  DashboardPanel,
} from "@agentsfleet/design-system";
import {
  CONNECTION_STATUS,
  useFleetEventStream,
} from "./useFleetEventStream";
import { useFleetThreadEntries, type FleetThreadEntry } from "./useFleetThreadEntries";
import type { EventRow } from "@/lib/api/events";
import { SenderLabelProvider } from "./FleetMessageRow";
import { SenderNamesProvider } from "./SenderNames";
import { senderNamesFrom, type SenderName } from "@/lib/events/sender-names";
import { FleetConnectionIndicator, useArrivalCue } from "./FleetConnectionIndicator";
import { useFleetPendingSends } from "./useFleetPendingSends";
import { useCurrentUser } from "@/lib/auth/client";
import { ACTOR } from "@/lib/events/event-summary";
import { useMessageDelivery } from "./useFleetMessageDelivery";
import { useFleetSteerQueue } from "./useFleetSteerQueue";
import { reportsOwnRun } from "./fleetReplyMessage";
import { FleetThreadViewport } from "./FleetThreadViewport";
import { FleetScopeProvider } from "./FleetScope";

export type FleetThreadProps = {
  modelLabel?: string;
  setupRevision?: string;
  setup?: ReactNode;
  workspaceId: string;
  fleetId: string;
  /** The console's own fleet — the name a fleet reply is labelled with. */
  senderLabel: string;
  /**
   * Server-rendered initial event rows. The browser holds no credential —
   * this data is fetched in the parent Server Component and passed as a
   * prop; live updates arrive over the cookie-authed SSE route handler.
   */
  initial: EventRow[];
  /**
   * The signed-in user as the server rendered the page. The pending-send
   * ledger is keyed by user, and the client learns the user only once its
   * auth script loads: a send made before then went to a ledger that vanished
   * when it did, taking its Resend with it.
   */
  viewer: string | null;
  /** The fleet's account members a thread can name, by the actor each one's
   * messages record; the server read them beside the thread. */
  senderNames: readonly SenderName[];
};

/**
 * Operator-facing chat surface backed by the durable event log. Wraps
 * `@assistant-ui/react` over `useFleetEventStream` + `postSteer` (the same-origin
 * `/live` steer route); `fleetMessageRenderers` paints each durable event as the
 * approved conversation row.
 *
 * The runtime is told the thread runs while the newest turn is one this tab
 * sent and its reply is still running, and every send goes through a queue (`useFleetSteerQueue`), so a run
 * never closes the composer: assistant-ui routes a send made mid-run to the
 * queue's steer lane instead of refusing it. That flag is what the viewport's
 * top anchor keys on to hold the viewer's place while their reply grows and
 * folds; any other sender's reply, another tab's included, leaves it off
 * (`reportsOwnRun`).
 */
export function FleetThread(props: FleetThreadProps) {
  const {stream, arrived, names, ledger, delivery, runtime, setupEventId} = useThreadRuntime(props);
  const settledLive = stream.connectionStatus === CONNECTION_STATUS.LIVE && !arrived;
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <FleetScopeProvider workspaceId={props.workspaceId} fleetId={props.fleetId}>
        <SenderLabelProvider senderLabel={props.senderLabel}>
          <SenderNamesProvider names={names}>
            <DashboardPanel id="fleet-chat-transcript" padding="none"
              className="relative flex min-h-0 flex-1 flex-col overflow-clip rounded-none border-0 bg-background">
              {settledLive ? null : <div data-testid="fleet-chat-header"
                className="pointer-events-none absolute right-lg top-sm z-20 rounded-md border border-border bg-background px-sm py-xs shadow-sm">
                <FleetConnectionIndicator status={stream.connectionStatus} arrived={arrived} />
              </div>}
              <FleetThreadViewport modelLabel={props.modelLabel} setup={props.setup} setupEventId={setupEventId}
                eventsCount={stream.events.length} connectionStatus={stream.connectionStatus}
                onRetry={stream.retryConnection} pending={ledger.pending} onResend={delivery.resend}
                onDismiss={ledger.writers.dismiss} onRestored={delivery.noteRestored} onDraft={delivery.noteDraft} />
            </DashboardPanel>
          </SenderNamesProvider>
        </SenderLabelProvider>
      </FleetScopeProvider>
    </AssistantRuntimeProvider>
  );
}

function useThreadRuntime({workspaceId, fleetId, initial, viewer, senderNames, setupRevision}: FleetThreadProps) {
  const stream = useFleetEventStream(workspaceId, fleetId, initial);
  // Connection chrome disappears after its arrival cue settles.
  const arrived = useArrivalCue(stream.connectionStatus);
  // The signed-in identity scopes the durable pending-send ledger.
  const { userId } = useCurrentUser();
  const subject = userId ?? viewer;
  const names = useMemo(() => senderNamesFrom(subject, senderNames), [subject, senderNames]);
  const ledger = useFleetPendingSends({ subject, workspaceId, fleetId });
  // Stable registry methods keep delivery independent of streamed frames.
  const delivery = useMessageDelivery({
    workspaceId,
    fleetId,
    sentAs: subject === null ? undefined : `${ACTOR.STEER_PREFIX}${subject}`,
    appendOptimistic: stream.appendOptimistic,
    reconcileOptimistic: stream.reconcileOptimistic,
    discardOptimistic: stream.discardOptimistic,
    writers: ledger.writers,
  });
  const { entries, convertEntry } = useFleetThreadEntries(stream.events, stream.convertEvent);
  const setupEventId = useMemo(() => stream.events.findLast(event => {
    const activity = readFleetActivity(event.custom?.requestJson ?? "");
    return event.status === FLEET_ACTIVITY.status && activity
      && activity.kind === ACTIVITY_KIND.setupSaved
      && activity.revision === setupRevision;
  })?.id, [stream.events, setupRevision]);
  // Queueing permits another send while this viewer's newest reply runs.
  const runtimeRef = useRef<AssistantRuntime | null>(null);
  const queue = useFleetSteerQueue(delivery.onNew, runtimeRef);
  const isRunning = useMemo(() => reportsOwnRun(stream.events, subject), [stream.events, subject]);
  const adapter = useMemo(
    () => ({ isRunning, messages: entries, convertMessage: convertEntry, onNew: delivery.onNew, queue }),
    [isRunning, entries, convertEntry, delivery.onNew, queue],
  );
  const runtime = useExternalStoreRuntime<FleetThreadEntry>(adapter);
  useEffect(() => {
    runtimeRef.current = runtime;
    return () => { runtimeRef.current = null; };
  }, [runtime]);
  return {stream, arrived, names, ledger, delivery, runtime, setupEventId};
}
