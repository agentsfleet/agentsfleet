"use client";

import { useCallback, useEffect, useMemo, useRef } from "react";
import { useRouter } from "next/navigation";
import {
  AssistantRuntimeProvider,
  ThreadPrimitive,
  useExternalStoreRuntime,
} from "@assistant-ui/react";
import {
  Button,
  Card,
  CardContent,
  CardHeader,
  Skeleton,
  WakePulse,
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
import type { ThreadEntry } from "@/lib/events/event-grouping";
import { useFleetThreadEntries } from "./useFleetThreadEntries";
import type { EventRow } from "@/lib/api/events";
import { SteerComposer } from "./SteerComposer";
import { renderFleetMessage } from "./fleetMessageRenderers";
import { FleetNameProvider } from "./FleetMessageRow";
import { FleetConnectionNotice } from "./FleetConnectionNotice";
import { FleetFailureBanner } from "./FleetFailureBanner";
import { failureBannerFor } from "@/lib/events/event-banner";
import { useFleetDeliveryFailure } from "./useFleetDeliveryFailure";
import { useNewMessageHandler } from "./useFleetMessageDelivery";

const PANEL_TITLE = "Chat";
const EMPTY_HINT =
  "Message this fleet or wait for its next trigger. Activity and outcomes appear here.";
const JUMP_TO_LATEST = "Jump to latest";
const JUMP_TO_LATEST_LABEL = "↓ latest";
const BACKFILL_LABEL = "Loading recent activity";

// The approved design carries the connection as a dot and a word, not a badge
// cluster — present enough to be honest, quiet enough not to compete with the
// conversation.
const STATUS_LABEL: Record<ConnectionStatus, string> = {
  [CONNECTION_STATUS.CONNECTING]: "Connecting…",
  [CONNECTION_STATUS.LIVE]: "Live",
  [CONNECTION_STATUS.RECONNECTING]: "Reconnecting…",
  [CONNECTION_STATUS.OFFLINE]: "Not live",
};

const STATUS_CLASS: Record<ConnectionStatus, string> = {
  [CONNECTION_STATUS.CONNECTING]: "text-info",
  [CONNECTION_STATUS.LIVE]: "text-pulse",
  [CONNECTION_STATUS.RECONNECTING]: "text-warning",
  [CONNECTION_STATUS.OFFLINE]: "text-destructive",
};

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
  // Pinned above the thread, derived from the same ordered array — so it
  // appears, counts up, and clears without any state of its own.
  const banner = useMemo(() => failureBannerFor(stream.events), [stream.events]);
  const runtime = useExternalStoreRuntime<ThreadEntry>({
    messages: entries,
    convertMessage: convertEntry,
    onNew: async (message) => {
      await deliverMessage(message);
    },
  });
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <FleetNameProvider fleetName={fleetName}>
        <div className="flex min-h-0 flex-1 flex-col gap-lg">
          <Card id="fleet-chat-transcript" aria-label="Fleet chat"
            className="flex min-h-0 flex-1 flex-col overflow-hidden bg-card p-0">
            <CardHeader
              data-testid="fleet-chat-header"
              className="flex flex-row items-center justify-between gap-md space-y-0 border-b border-border px-xl py-0"
            >
              <h2 className="border-b-2 border-pulse py-lg font-mono text-sm font-medium text-foreground">
                {PANEL_TITLE}</h2>
              <ConnectionIndicator status={stream.connectionStatus} />
            </CardHeader>
            <CardContent className="flex min-h-0 flex-1 flex-col p-0">
              <FleetConnectionNotice status={stream.connectionStatus} onRetry={stream.retryConnection} />
              <FleetFailureBanner banner={banner} />
              <ThreadViewport eventsCount={stream.events.length}
                connectionStatus={stream.connectionStatus} />
            </CardContent>
          </Card>
          <SteerComposer failureKind={failedDelivery?.kind ?? null} onRetry={retryFailedDelivery} />
        </div>
      </FleetNameProvider>
    </AssistantRuntimeProvider>
  );
}

// ── internals ────────────────────────────────────────────────────────────

function ConnectionIndicator({ status }: { status: ConnectionStatus }) {
  const live = status === CONNECTION_STATUS.LIVE;
  return (
    <span
      aria-label={`Connection status: ${STATUS_LABEL[status]}`}
      className={cn("inline-flex items-center gap-sm font-mono text-label", STATUS_CLASS[status])}
      data-connection={status}
    >
      <WakePulse
        live={live}
        className="inline-block h-2 w-2 rounded-full bg-current"
        aria-hidden="true"
      />
      {STATUS_LABEL[status]}
    </span>
  );
}

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
}: {
  eventsCount: number;
  connectionStatus: ConnectionStatus;
}) {
  const isAwaitingFirstFrames =
    eventsCount === 0 &&
    (connectionStatus === CONNECTION_STATUS.CONNECTING ||
      connectionStatus === CONNECTION_STATUS.RECONNECTING);
  const isIdleEmpty = eventsCount === 0 && connectionStatus === CONNECTION_STATUS.LIVE;
  return (
    <ThreadPrimitive.Root
      className={cn(
        "relative flex min-h-0 flex-1 flex-col bg-surface-deep",
      )}
    >
      {/* The conversation is the only thing on this page that scrolls. Its
          own overflow is what keeps the composer below it on screen. */}
      <ThreadPrimitive.Viewport
        autoScroll
        className="min-h-0 flex-1 overflow-y-auto"
        role="log"
        aria-live="polite"
        aria-label={PANEL_TITLE}
      >
        {isAwaitingFirstFrames ? <BackfillSkeleton /> : null}
        {isIdleEmpty ? (
          <p className="px-xl py-lg text-sm text-muted-foreground">{EMPTY_HINT}</p>
        ) : null}
        <ThreadPrimitive.Messages>{renderFleetMessage}</ThreadPrimitive.Messages>
      </ThreadPrimitive.Viewport>
      <ThreadPrimitive.ScrollToBottom asChild>
        <Button
          variant="secondary"
          size="sm"
          aria-label={JUMP_TO_LATEST}
          className={cn(
            "absolute bottom-md right-md font-mono text-label",
            "disabled:invisible disabled:pointer-events-none",
          )}
        >
          {JUMP_TO_LATEST_LABEL}
        </Button>
      </ThreadPrimitive.ScrollToBottom>
    </ThreadPrimitive.Root>
  );
}

function BackfillSkeleton() {
  return (
    <div
      className="flex flex-col gap-md px-xl py-lg"
      aria-label={BACKFILL_LABEL}
      data-testid="backfill-skeleton"
    >
      <Skeleton className="h-12 w-full rounded-md" />
      <Skeleton className="h-12 w-3/4 rounded-md" />
      <Skeleton className="h-12 w-2/3 rounded-md" />
    </div>
  );
}
