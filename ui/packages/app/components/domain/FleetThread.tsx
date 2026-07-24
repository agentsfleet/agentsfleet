"use client";

import { useCallback, useEffect, useRef } from "react";
import { useRouter } from "next/navigation";
import {
  AssistantRuntimeProvider,
  ThreadPrimitive,
  useExternalStoreRuntime,
} from "@assistant-ui/react";
import {
  Button,
  DashboardPanel,
  DashboardPanelFooter,
  DashboardPanelHeader,
  DashboardPanelTitle,
  Skeleton,
  cn,
} from "@agentsfleet/design-system";
import {
  CONNECTION_STATUS,
  useFleetEventStream,
  type ConnectionStatus,
  type FleetEvent,
  type FleetEventStatus,
} from "./useFleetEventStream";
import { AGENTSFLEET_EVENT_STATUS } from "@/lib/streaming/fleet-stream-frames";
import { useFleetThreadEntries, type FleetThreadEntry } from "./useFleetThreadEntries";
import type { EventRow } from "@/lib/api/events";
import { SteerComposer } from "./SteerComposer";
import { renderFleetMessage } from "./fleetMessageRenderers";
import { FleetNameProvider } from "./FleetMessageRow";
import { FleetConnectionNotice } from "./FleetConnectionNotice";
import { FleetConnectionIndicator } from "./FleetConnectionIndicator";
import {
  useFleetDeliveryFailure,
  type DeliveryFailureKind,
} from "./useFleetDeliveryFailure";
import { useNewMessageHandler } from "./useFleetMessageDelivery";

const PANEL_TITLE = "Chat";
const EMPTY_HINT =
  "Message this fleet or wait for its next trigger. Activity and outcomes appear here.";
const JUMP_TO_LATEST = "Jump to latest";
const JUMP_TO_LATEST_LABEL = "↓ latest";
const BACKFILL_LABEL = "Loading recent activity";

const TERMINAL_EVENT_STATUSES: ReadonlySet<FleetEventStatus> = new Set([
  AGENTSFLEET_EVENT_STATUS.PROCESSED,
  AGENTSFLEET_EVENT_STATUS.AGENT_ERROR,
  AGENTSFLEET_EVENT_STATUS.GATE_BLOCKED,
]);

export type FleetThreadProps = {
  workspaceId: string;
  fleetId: string;
  /** The console's own fleet — the name a fleet reply is labelled with. */
  fleetName: string;
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
export function FleetThread({ workspaceId, fleetId, fleetName, initial }: FleetThreadProps) {
  const stream = useFleetEventStream(workspaceId, fleetId, initial);
  const {
    failedDelivery,
    setFailedDelivery,
    clearFailedDelivery,
  } = useFleetDeliveryFailure(fleetId);
  useRefreshSummariesOnCompletion(initial, stream.events);
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
  const runtime = useExternalStoreRuntime<FleetThreadEntry>({
    messages: entries,
    convertMessage: convertEntry,
    onNew: async (message) => {
      await deliverMessage(message);
    },
  });
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <FleetNameProvider fleetName={fleetName}>
        <DashboardPanel
          id="fleet-chat-transcript"
          aria-label="Fleet chat"
          padding="none"
          className="flex min-h-0 flex-1 flex-col overflow-hidden bg-card"
        >
          <DashboardPanelHeader
            data-testid="fleet-chat-header"
            className="shrink-0 border-b border-border px-lg py-md sm:px-xl"
          >
            <DashboardPanelTitle className="text-body font-medium">{PANEL_TITLE}</DashboardPanelTitle>
            <FleetConnectionIndicator status={stream.connectionStatus} />
          </DashboardPanelHeader>
          {stream.connectionStatus === CONNECTION_STATUS.OFFLINE ? (
            <FleetConnectionNotice status={stream.connectionStatus} onRetry={stream.retryConnection} />
          ) : null}
          <ThreadViewport
            eventsCount={stream.events.length}
            connectionStatus={stream.connectionStatus}
            failureKind={failedDelivery?.kind ?? null}
            onRetry={retryFailedDelivery}
          />
        </DashboardPanel>
      </FleetNameProvider>
    </AssistantRuntimeProvider>
  );
}

// ── internals ────────────────────────────────────────────────────────────

function useRefreshSummariesOnCompletion(initial: EventRow[], events: FleetEvent[]) {
  const router = useRouter();
  const terminalEventIds = useRef(
    new Set([
      ...events
        .filter((event) => TERMINAL_EVENT_STATUSES.has(event.status))
        .map((event) => event.id),
      ...initial
        .filter((event) => event.status !== AGENTSFLEET_EVENT_STATUS.RECEIVED)
        .map((event) => event.event_id),
    ]),
  );
  useEffect(() => {
    let completed = false;
    for (const event of events) {
      if (
        TERMINAL_EVENT_STATUSES.has(event.status) &&
        terminalEventIds.current.has(event.id) === false
      ) {
        terminalEventIds.current.add(event.id);
        completed = true;
      }
    }
    if (completed) router.refresh();
  }, [events, router]);
}

function ThreadViewport({
  eventsCount,
  connectionStatus,
  failureKind,
  onRetry,
}: {
  eventsCount: number;
  connectionStatus: ConnectionStatus;
  failureKind: DeliveryFailureKind | null;
  onRetry: () => void;
}) {
  const isAwaitingFirstFrames =
    eventsCount === 0 &&
    (connectionStatus === CONNECTION_STATUS.CONNECTING ||
      connectionStatus === CONNECTION_STATUS.RECONNECTING);
  const isIdleEmpty = eventsCount === 0 && connectionStatus === CONNECTION_STATUS.LIVE;
  return (
    <ThreadPrimitive.Root
      className="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-surface-deep"
    >
      {/* The conversation is the only thing on this page that scrolls. Its
          own overflow keeps the centered composer visible on screen. */}
      <ThreadPrimitive.Viewport
        autoScroll
        className="min-h-0 flex-1 overflow-y-auto px-lg sm:px-xl"
        role="presentation"
      >
        <div className="relative min-h-full w-full">
          <div
            role="log"
            aria-live="polite"
            aria-label={PANEL_TITLE}
            className="mx-auto flex min-h-full w-full max-w-6xl flex-col justify-end py-lg"
          >
            {isAwaitingFirstFrames ? <BackfillSkeleton /> : null}
            {isIdleEmpty ? (
              <p className="px-sm py-lg text-sm text-muted-foreground">{EMPTY_HINT}</p>
            ) : null}
            <ThreadPrimitive.Messages>{renderFleetMessage}</ThreadPrimitive.Messages>
          </div>
          <ThreadPrimitive.ScrollToBottom asChild>
            <Button
              variant="secondary"
              size="sm"
              aria-label={JUMP_TO_LATEST}
              className={cn(
                "absolute bottom-md right-0 z-20 font-mono text-label",
                "disabled:invisible disabled:pointer-events-none",
              )}
            >
              {JUMP_TO_LATEST_LABEL}
            </Button>
          </ThreadPrimitive.ScrollToBottom>
        </div>
      </ThreadPrimitive.Viewport>
      <DashboardPanelFooter
        data-testid="fleet-chat-footer"
        className="relative mx-auto mt-0 w-full max-w-6xl shrink-0 border-0 bg-surface-deep px-0 pb-md pt-sm"
      >
        <SteerComposer failureKind={failureKind} onRetry={onRetry} />
      </DashboardPanelFooter>
    </ThreadPrimitive.Root>
  );
}

function BackfillSkeleton() {
  return (
    <div
      className="flex w-full flex-col gap-md py-lg"
      aria-label={BACKFILL_LABEL}
      data-testid="backfill-skeleton"
    >
      <Skeleton className="h-12 w-full rounded-md" />
      <Skeleton className="h-12 w-3/4 rounded-md" />
      <Skeleton className="h-12 w-2/3 rounded-md" />
    </div>
  );
}
