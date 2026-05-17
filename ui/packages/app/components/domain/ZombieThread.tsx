"use client";

import { useCallback, useState } from "react";
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
} from "@usezombie/design-system";
import {
  CONNECTION_STATUS,
  useZombieEventStream,
  type ConnectionStatus,
  type RetryState,
  type ZombieEvent,
} from "./useZombieEventStream";
import { steerZombie } from "@/lib/api/zombies";
import { RETRY_DEFAULTS, type RetryReason } from "@/lib/api/retry";
import { SteerComposer } from "./SteerComposer";
import { renderZombieMessage } from "./zombieMessageRenderers";

const PANEL_TITLE = "Live activity";
const EMPTY_HINT =
  "Waiting for activity. Tool calls, chunks, and completions appear here as the zombie runs.";

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

type SteerRetryState = {
  phase: "steer";
  attempt: number;
  max: number;
  reason: RetryReason;
} | null;

type AnyRetryState = RetryState | SteerRetryState;

export type ZombieThreadProps = {
  workspaceId: string;
  zombieId: string;
  /**
   * Server-acquired API token forwarded from the parent server component.
   * Per `docs/AUTH.md`, dashboard client components do not call Clerk's
   * `getToken` directly — the token is resolved server-side and passed
   * down as a prop.
   */
  token: string | null;
};

/**
 * Operator-facing chat surface backed by the durable event log. Wraps
 * `@assistant-ui/react` over `useZombieEventStream` (D3) + `steerZombie`
 * (D6); custom renderers under `zombieMessageRenderers` paint the
 * per-actor system chips (webhook / cron / continuation / config_reload
 * / gate_blocked) plus the streaming + optimistic states.
 */
export function ZombieThread({ workspaceId, zombieId, token }: ZombieThreadProps) {
  const stream = useZombieEventStream(workspaceId, zombieId, token);
  const [steerRetry, setSteerRetry] = useState<SteerRetryState>(null);
  const onNew = useNewMessageHandler({
    workspaceId,
    zombieId,
    token,
    stream,
    onSteerRetry: setSteerRetry,
  });
  const runtime = useExternalStoreRuntime<ZombieEvent>({
    messages: stream.events,
    convertMessage: stream.convertEvent,
    isRunning: stream.isRunning,
    onNew,
  });
  const activeRetry: AnyRetryState = steerRetry ?? stream.retryState;
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
          <RetryLine state={activeRetry} />
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

/**
 * Visible retry counter ("Retrying backfill… attempt 2 of 3 · 503"),
 * rendered between header and viewport while either the backfill GET
 * or the steer POST is mid-retry. Mirrors the CLI's visible-retry
 * UX so agents + humans observe the same "trying N of M" signal in
 * both surfaces. `role="status" aria-live="polite"` so screen readers
 * announce the attempt updates without interrupting the user.
 */
function RetryLine({ state }: { state: AnyRetryState }) {
  if (state === null) return null;
  return (
    <output
      aria-live="polite"
      className={cn(
        "block border-b border-border bg-muted px-xl py-xs",
        "font-mono text-label leading-mono text-evidence",
      )}
      data-retry-phase={state.phase}
    >
      Retrying {state.phase}… attempt {state.attempt} of {state.max} · {state.reason}
    </output>
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
          {renderZombieMessage}
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

type NewHandlerCtx = {
  workspaceId: string;
  zombieId: string;
  token: string | null;
  stream: ReturnType<typeof useZombieEventStream>;
  onSteerRetry: (state: SteerRetryState) => void;
};

function useNewMessageHandler({
  workspaceId,
  zombieId,
  token,
  stream,
  onSteerRetry,
}: NewHandlerCtx): (msg: AppendMessage) => Promise<void> {
  return useCallback(
    async (msg: AppendMessage) => {
      if (!token) return;
      const text = extractMessageText(msg);
      if (text.length === 0) return;
      const tempId = stream.appendOptimistic(text, OPTIMISTIC_ACTOR);
      try {
        const { event_id } = await steerZombie(
          workspaceId,
          zombieId,
          text,
          token,
          {
            onRetry: ({ attempt, reason }) => {
              onSteerRetry({
                phase: "steer",
                attempt,
                max: RETRY_DEFAULTS.maxAttempts,
                reason,
              });
            },
            onAttempt: ({ terminal }) => {
              if (terminal) onSteerRetry(null);
            },
          },
        );
        stream.reconcileOptimistic(tempId, event_id);
      } catch {
        // Failed steers stay optimistic until the user re-steers or
        // reloads; explicit failure surfacing is handled by a future
        // task (the steer endpoint's 4xx returns surface in the SSE
        // stream as `gate_blocked` events). The terminal `onAttempt`
        // already cleared the retry indicator.
      }
    },
    [workspaceId, zombieId, token, stream, onSteerRetry],
  );
}

function extractMessageText(msg: AppendMessage): string {
  for (const part of msg.content) {
    if (part.type === "text") return part.text;
  }
  return "";
}
