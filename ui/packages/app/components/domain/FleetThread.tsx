"use client";

import { useCallback } from "react";
import {
  AssistantRuntimeProvider,
  ThreadPrimitive,
  useExternalStoreRuntime,
  type AppendMessage,
} from "@assistant-ui/react";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Skeleton,
  WakePulse,
  cn,
} from "@agentsfleet/design-system";
import {
  CONNECTION_STATUS,
  useFleetEventStream,
  type ConnectionStatus,
  type FleetEvent,
} from "./useFleetEventStream";
import type { EventRow } from "@/lib/api/events";
import { steerFleetAction } from "@/app/(dashboard)/fleets/actions";
import { SteerComposer } from "./SteerComposer";
import { renderFleetMessage } from "./fleetMessageRenderers";

const PANEL_TITLE = "Live activity";
const EMPTY_HINT =
  "Waiting for activity. Tool calls, chunks, and completions appear here as the fleet runs.";

const STATUS_LABEL: Record<ConnectionStatus, string> = {
  [CONNECTION_STATUS.CONNECTING]: "Connecting…",
  [CONNECTION_STATUS.LIVE]: "Live",
  [CONNECTION_STATUS.RECONNECTING]: "Reconnecting…",
};

const STATUS_VARIANT: Record<ConnectionStatus, "cyan" | "live" | "amber"> = {
  [CONNECTION_STATUS.CONNECTING]: "cyan",
  [CONNECTION_STATUS.LIVE]: "live",
  [CONNECTION_STATUS.RECONNECTING]: "amber",
};

// Placeholder actor used on optimistic user messages until the SSE
// stream's matching `EVENT_RECEIVED` lands and reconciliation runs.
// The server's actor (the authenticated user's email) replaces this.
const OPTIMISTIC_ACTOR = "steer:pending";

export type FleetThreadProps = {
  workspaceId: string;
  fleetId: string;
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
 * Server Action; custom renderers under `fleetMessageRenderers` paint the
 * per-actor system chips (webhook / cron / continuation / config_reload
 * / gate_blocked) plus the streaming, optimistic, and failed states.
 */
export function FleetThread({ workspaceId, fleetId, initial }: FleetThreadProps) {
  const stream = useFleetEventStream(workspaceId, fleetId, initial);
  // Pass the registry methods (each `useCallback([fleetId])`-stable), not
  // the whole `stream` object — `stream` is a fresh reference on every SSE
  // frame, so listing it would rebuild `onNew` per frame for no benefit.
  const onNew = useNewMessageHandler({
    workspaceId,
    fleetId,
    appendOptimistic: stream.appendOptimistic,
    reconcileOptimistic: stream.reconcileOptimistic,
    markOptimisticFailed: stream.markOptimisticFailed,
  });
  const runtime = useExternalStoreRuntime<FleetEvent>({
    messages: stream.events,
    convertMessage: stream.convertEvent,
    isRunning: stream.isRunning,
    onNew,
  });
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <Card aria-label="Live activity stream">
        <CardHeader className="flex flex-row items-center justify-between gap-md space-y-0 py-lg">
            <CardTitle className="flex items-center gap-md text-sm font-medium">
              <WakePulse
                live={stream.connectionStatus === CONNECTION_STATUS.LIVE}
                className="inline-block h-2 w-2 rounded-full bg-pulse"
                aria-hidden="true"
              />
              {PANEL_TITLE}
            </CardTitle>
            <div className="flex items-center gap-md">
              <span className="font-mono text-label text-muted-foreground">
                {stream.events.length} events
              </span>
              <Badge variant={STATUS_VARIANT[stream.connectionStatus]}>
                {STATUS_LABEL[stream.connectionStatus]}
              </Badge>
            </div>
          </CardHeader>
          <CardContent className="p-0">
            <ThreadViewport
              eventsCount={stream.events.length}
              connectionStatus={stream.connectionStatus}
            />
            <SteerComposer isRunning={stream.isRunning} />
          </CardContent>
      </Card>
    </AssistantRuntimeProvider>
  );
}

// ── internals ────────────────────────────────────────────────────────────

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
        "relative flex flex-col bg-surface-deep",
        "border-y border-border",
      )}
    >
      <ThreadPrimitive.Viewport
        autoScroll
        className="flex-1"
        role="log"
        aria-live="polite"
        aria-label={PANEL_TITLE}
      >
        {isAwaitingFirstFrames ? <BackfillSkeleton /> : null}
        {isIdleEmpty ? (
          <p className="px-xl py-lg text-sm text-muted-foreground">
            {EMPTY_HINT}
          </p>
        ) : null}
        <ThreadPrimitive.Messages>
          {renderFleetMessage}
        </ThreadPrimitive.Messages>
      </ThreadPrimitive.Viewport>
      <ThreadPrimitive.ScrollToBottom asChild>
        <Button
          variant="secondary"
          size="sm"
          aria-label="Jump to latest"
          className={cn(
            "absolute bottom-md right-md font-mono text-label",
            "disabled:invisible disabled:pointer-events-none",
          )}
        >
          ↓ latest
        </Button>
      </ThreadPrimitive.ScrollToBottom>
    </ThreadPrimitive.Root>
  );
}

function BackfillSkeleton() {
  return (
    <div
      className="flex flex-col gap-md px-xl py-lg"
      aria-label="Loading recent activity"
      data-testid="backfill-skeleton"
    >
      <Skeleton className="h-12 w-full rounded-md" />
      <Skeleton className="h-12 w-3/4 rounded-md" />
      <Skeleton className="h-12 w-2/3 rounded-md" />
    </div>
  );
}

type StreamApi = ReturnType<typeof useFleetEventStream>;
type NewHandlerCtx = {
  workspaceId: string;
  fleetId: string;
  appendOptimistic: StreamApi["appendOptimistic"];
  reconcileOptimistic: StreamApi["reconcileOptimistic"];
  markOptimisticFailed: StreamApi["markOptimisticFailed"];
};

function useNewMessageHandler({
  workspaceId,
  fleetId,
  appendOptimistic,
  reconcileOptimistic,
  markOptimisticFailed,
}: NewHandlerCtx): (msg: AppendMessage) => Promise<void> {
  return useCallback(
    async (msg: AppendMessage) => {
      const text = extractMessageText(msg);
      if (text.length === 0) return;
      const tempId = appendOptimistic(text, OPTIMISTIC_ACTOR);
      try {
        const result = await steerFleetAction(workspaceId, fleetId, text);
        if (result.ok) {
          reconcileOptimistic(tempId, result.data.event_id);
        } else {
          markOptimisticFailed(tempId);
        }
      } catch {
        // The Server Action's RPC transport itself failed (offline, or the
        // action invocation errored) — surface the same `failed` row the
        // ok:false path produces so the user knows the steer didn't land.
        markOptimisticFailed(tempId);
      }
    },
    [workspaceId, fleetId, appendOptimistic, reconcileOptimistic, markOptimisticFailed],
  );
}

function extractMessageText(msg: AppendMessage): string {
  for (const part of msg.content) {
    if (part.type === "text") return part.text;
  }
  return "";
}
