"use client";

import { useCallback } from "react";
import {
  AssistantRuntimeProvider,
  useExternalStoreRuntime,
} from "@assistant-ui/react";
import {
  DashboardPanel,
  DashboardPanelHeader,
} from "@agentsfleet/design-system";
import {
  CONNECTION_STATUS,
  useFleetEventStream,
} from "./useFleetEventStream";
import { useFleetThreadEntries, type FleetThreadEntry } from "./useFleetThreadEntries";
import type { EventRow } from "@/lib/api/events";
import { SenderLabelProvider } from "./FleetMessageRow";
import { FleetConnectionNotice } from "./FleetConnectionNotice";
import { FleetConnectionIndicator, useArrivalCue } from "./FleetConnectionIndicator";
import {
  useFleetDeliveryFailure,
} from "./useFleetDeliveryFailure";
import { useNewMessageHandler } from "./useFleetMessageDelivery";
import { FleetThreadViewport } from "./FleetThreadViewport";
import { STATUS_OPTIMISTIC } from "./fleetMessageStatus";

export type FleetThreadProps = {
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
};

/**
 * Operator-facing chat surface backed by the durable event log. Wraps
 * `@assistant-ui/react` over `useFleetEventStream` + the `steerFleetAction`
 * Server Action; `fleetMessageRenderers` paints each durable event as the
 * approved conversation row.
 *
 * The runtime is deliberately never told the thread is running. In this
 * library `isRunning` means "disable the composer", and a working fleet is
 * not a reason to stop an operator from steering it — the fleet's own event
 * stream serialises what arrives. The working state is rendered from our own
 * event statuses instead.
 */
export function FleetThread({
  workspaceId,
  fleetId,
  senderLabel,
  initial,
}: FleetThreadProps) {
  const stream = useFleetEventStream(workspaceId, fleetId, initial);
  // The header row is chrome that earns its space only while the stream is not
  // yet fine. `arrived` keeps it for the length of the arrival cue so the
  // operator who WAS waiting gets the confirmation, and then the row goes —
  // its disappearance being the steady-state signal that nothing is wrong.
  const arrived = useArrivalCue(stream.connectionStatus);
  const settledLive = stream.connectionStatus === CONNECTION_STATUS.LIVE && !arrived;
  const {
    failedDelivery,
    setFailedDelivery,
    clearFailedDelivery,
  } = useFleetDeliveryFailure(fleetId);
  // Pass the registry methods (each `useCallback([fleetId])`-stable), not
  // the whole `stream` object — `stream` is a fresh reference on every SSE
  // frame, so listing it would rebuild `onNew` per frame for no benefit.
  const deliverMessage = useNewMessageHandler({
    workspaceId,
    fleetId,
    appendOptimistic: stream.appendOptimistic,
    reconcileOptimistic: stream.reconcileOptimistic,
    markOptimisticFailed: stream.markOptimisticFailed,
    onFailure: setFailedDelivery,
  });
  const { discardOptimistic } = stream;
  const retryFailedDelivery = useCallback(() => {
    if (!failedDelivery) return;
    const { message, tempId } = failedDelivery;
    clearFailedDelivery();
    // The retry re-submits as a fresh optimistic row; the stale failed row
    // must leave first or each attempt stacks a duplicate of the message.
    discardOptimistic(tempId);
    void deliverMessage(message);
  }, [clearFailedDelivery, deliverMessage, discardOptimistic, failedDelivery]);
  // Runs of identical activity render as one expandable row. Grouping is a
  // pure view over the array the stream already ordered — it never reorders,
  // drops, or renames an event, so a group can always hand back what it hid.
  const { entries, convertEntry } = useFleetThreadEntries(stream.events, stream.convertEvent);
  const newestEvent = stream.events.at(-1);
  const pendingMessageId = newestEvent?.status === STATUS_OPTIMISTIC ? newestEvent.id : null;
  const runtime = useExternalStoreRuntime<FleetThreadEntry>({
    messages: entries,
    convertMessage: convertEntry,
    onNew: async (message) => {
      await deliverMessage(message);
    },
  });
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <SenderLabelProvider senderLabel={senderLabel}>
        <DashboardPanel
          id="fleet-chat-transcript"
          aria-label="Fleet chat"
          padding="none"
          className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-none border-0 bg-background"
        >
          {/*
            * The header speaks only when the stream is not fine.
            *
            * It carried the word "Chat" directly under a tab already reading
            * "Chat", and a steady "Live" that said nothing on the overwhelming
            * majority of loads. A transcript is the page's content; labelling
            * it costs a row and tells the operator what they can see.
            *
            * What is worth saying is the exception, so connecting, reconnecting
            * and offline still render here — and OFFLINE additionally gets the
            * notice below, with its retry. `PANEL_TITLE` stays as the scroll
            * region's accessible name, where it is the only name that region
            * has.
            */}
          {settledLive ? null : (
            <DashboardPanelHeader
              data-testid="fleet-chat-header"
              className="shrink-0 border-b border-border px-lg py-md sm:px-xl"
            >
              <FleetConnectionIndicator status={stream.connectionStatus} arrived={arrived} />
            </DashboardPanelHeader>
          )}
          {stream.connectionStatus === CONNECTION_STATUS.OFFLINE ? (
            <FleetConnectionNotice status={stream.connectionStatus} onRetry={stream.retryConnection} />
          ) : null}
          <FleetThreadViewport
            eventsCount={stream.events.length}
            pendingMessageId={pendingMessageId}
            connectionStatus={stream.connectionStatus}
            failureKind={failedDelivery?.kind ?? null}
            onRetry={retryFailedDelivery}
          />
        </DashboardPanel>
      </SenderLabelProvider>
    </AssistantRuntimeProvider>
  );
}
